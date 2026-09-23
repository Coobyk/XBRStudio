use std::collections::{HashMap, HashSet};

use image::{Rgba, RgbaImage};
use xbrz::scale_rgba;

use crate::model::{FaceRect, ModelFace};

pub const MAX_FACTOR: u32 = 6;

#[derive(Debug, PartialEq, Eq)]
pub struct UpscaleConfig {
    pub factor: u32,
    pub stitch_faces: bool,
}

impl UpscaleConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=MAX_FACTOR).contains(&self.factor) {
            return Err(format!("factor must be between 2 and {MAX_FACTOR}"));
        }
        Ok(())
    }
}

/// Add a 1-pixel self-tiling border: edges copy the opposite edge, corners
/// copy the opposite corner (padded(x, y) = source((x + w - 1) % w, ...)).
pub(crate) fn wrap_tile(image: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let mut padded = RgbaImage::new(width + 2, height + 2);
    for y in 0..height + 2 {
        for x in 0..width + 2 {
            let source_x = (x + width - 1) % width;
            let source_y = (y + height - 1) % height;
            padded.put_pixel(x, y, *image.get_pixel(source_x, source_y));
        }
    }
    padded
}

/// Upscale with a wrapped 1px border so tiled textures (blocks) get correct
/// edge context, then shave `factor` output pixels (the scaled border) off
/// every side. Output dimensions match a plain upscale exactly.
pub fn upscale_wrapped(image: &RgbaImage, factor: u32) -> Result<RgbaImage, String> {
    let config = UpscaleConfig {
        factor,
        stitch_faces: false,
    };
    config.validate()?;
    if image.width() == 0 || image.height() == 0 {
        return Err("cannot wrap a zero-size texture".into());
    }

    let padded = wrap_tile(image);
    let upscaled = upscale_image(&padded, None, &config)?;
    let (width, height) = (image.width() * factor, image.height() * factor);
    Ok(image::imageops::crop_imm(&upscaled, factor, factor, width, height).to_image())
}

pub fn upscale_image(
    image: &RgbaImage,
    faces: Option<&[ModelFace]>,
    config: &UpscaleConfig,
) -> Result<RgbaImage, String> {
    config.validate()?;

    let (source_width, source_height) = (image.width(), image.height());
    let (output_width, output_height) = (
        source_width * config.factor,
        source_height * config.factor,
    );

    if !config.stitch_faces {
        let scaled = scale_rgba(
            image.as_raw(),
            source_width as usize,
            source_height as usize,
            config.factor as usize,
        );

        return RgbaImage::from_raw(output_width, output_height, scaled)
            .ok_or_else(|| "xBRZ produced an invalid image buffer".to_string());
    }

    let faces = faces.ok_or_else(|| "stitching requires model faces".to_string())?;
    if faces.is_empty() {
        return Err("stitching requires at least one model face".to_string());
    }

    upscale_box_faces(image, faces, config.factor)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Cube-edge adjacency, derived from `ModelPart.Cube` polygon vertex/UV
/// assignment. Each face edge (top/bottom run left→right, left/right run
/// top→bottom) maps to the neighbor face across the shared 3D edge, the
/// neighbor's corresponding edge, and whether that edge traverses the shared
/// line in the opposite direction.
fn neighbor_edge(face: &str, edge: Edge) -> Option<(&'static str, Edge, bool)> {
    use Edge::*;
    Some(match (face, edge) {
        ("down", Top) => ("south", Top, true),
        ("down", Bottom) => ("north", Top, false),
        ("down", Left) => ("west", Top, false),
        ("down", Right) => ("east", Top, true),
        ("up", Top) => ("south", Bottom, true),
        ("up", Bottom) => ("north", Bottom, false),
        ("up", Left) => ("west", Bottom, false),
        ("up", Right) => ("east", Bottom, true),
        ("west", Top) => ("down", Left, false),
        ("west", Bottom) => ("up", Left, false),
        ("west", Left) => ("south", Right, false),
        ("west", Right) => ("north", Left, false),
        ("north", Top) => ("down", Bottom, false),
        ("north", Bottom) => ("up", Bottom, false),
        ("north", Left) => ("west", Right, false),
        ("north", Right) => ("east", Left, false),
        ("east", Top) => ("down", Right, true),
        ("east", Bottom) => ("up", Right, true),
        ("east", Left) => ("north", Right, false),
        ("east", Right) => ("south", Left, false),
        ("south", Top) => ("down", Top, true),
        ("south", Bottom) => ("up", Top, true),
        ("south", Left) => ("east", Right, false),
        ("south", Right) => ("west", Left, false),
        _ => return None,
    })
}

fn sample_clamped(image: &RgbaImage, x: i64, y: i64) -> Rgba<u8> {
    let x = x.clamp(0, image.width() as i64 - 1) as u32;
    let y = y.clamp(0, image.height() as i64 - 1) as u32;
    *image.get_pixel(x, y)
}

/// Sample `rect`'s `edge` at parameter `i` of an edge that is `len` long on
/// the asking face (param 0 = left/top end). `rev` flips the direction first;
/// the parameter is then mapped proportionally onto the neighbor's own edge.
/// `depth` walks that far into the rect from the shared edge (0 = the edge
/// itself), clamped so sampling never leaves the rect.
fn sample_edge(
    image: &RgbaImage,
    rect: &FaceRect,
    edge: Edge,
    rev: bool,
    i: i32,
    len: usize,
    depth: u32,
) -> Rgba<u8> {
    let len = len.max(1);
    let i = i.clamp(0, len as i32 - 1) as usize;
    let j = if rev { len - 1 - i } else { i };
    let n_len = match edge {
        Edge::Top | Edge::Bottom => rect.width.max(1) as usize,
        Edge::Left | Edge::Right => rect.height.max(1) as usize,
    };
    let p = if len <= 1 {
        0
    } else {
        j * (n_len - 1) / (len - 1)
    };
    let d_x = depth.min(rect.width.saturating_sub(1));
    let d_y = depth.min(rect.height.saturating_sub(1));
    let (x, y) = match edge {
        Edge::Top => (rect.x + p as u32, rect.y + d_y),
        Edge::Bottom => (
            rect.x + p as u32,
            rect.bottom().saturating_sub(1 + d_y),
        ),
        Edge::Left => (rect.x + d_x, rect.y + p as u32),
        Edge::Right => (
            rect.right().saturating_sub(1 + d_x),
            rect.y + p as u32,
        ),
    };
    sample_clamped(image, x as i64, y as i64)
}

fn neighbor_or_wrap(
    source: &RgbaImage,
    face: &ModelFace,
    group: &HashMap<&str, &ModelFace>,
    edge: Edge,
    i: i32,
    len: usize,
    depth: u32,
) -> Rgba<u8> {
    if let Some((name, n_edge, rev)) = neighbor_edge(&face.face, edge) {
        if let Some(neighbor) = group.get(name) {
            return sample_edge(source, &neighbor.rect, n_edge, rev, i, len, depth);
        }
    }
    // No neighbor face: use the atlas pixels just outside this edge (the same
    // context whole-image xBRZ sees). Self-wrapping the opposite edge invents
    // seams on sparse flat-cube faces (axolotl gills/legs).
    sample_atlas_outside(source, &face.rect, edge, i, len, depth)
}

/// Sample `depth` steps outward from `rect`'s `edge` (0 = the adjacent atlas
/// pixel outside the face), parameterized along the edge like `sample_edge`.
fn sample_atlas_outside(
    source: &RgbaImage,
    rect: &FaceRect,
    edge: Edge,
    i: i32,
    len: usize,
    depth: u32,
) -> Rgba<u8> {
    let len = len.max(1);
    let i = i.clamp(0, len as i32 - 1) as usize;
    let p = if len <= 1 {
        0
    } else {
        i * (match edge {
            Edge::Top | Edge::Bottom => rect.width,
            Edge::Left | Edge::Right => rect.height,
        }
        .max(1) as usize
            - 1)
            / (len - 1)
    };
    let (x, y) = match edge {
        Edge::Top => (rect.x + p as u32, rect.y.saturating_sub(1 + depth)),
        Edge::Bottom => (rect.x + p as u32, rect.bottom() + depth),
        Edge::Left => (rect.x.saturating_sub(1 + depth), rect.y + p as u32),
        Edge::Right => (rect.right() + depth, rect.y + p as u32),
    };
    sample_clamped(source, x as i64, y as i64)
}

#[derive(Clone, Copy)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Corner pixel at along-edge index `i` (may sit outside the face; clamped)
/// and `depth` steps into the neighbor from the shared edge. Prefers the
/// neighbor across the horizontal edge (top/bottom), then the vertical-edge
/// neighbor, then the atlas pixel diagonally outside the face.
fn corner_pixel(
    source: &RgbaImage,
    face: &ModelFace,
    group: &HashMap<&str, &ModelFace>,
    corner: Corner,
    i: i32,
    depth: u32,
) -> Rgba<u8> {
    let w = face.rect.width as usize;
    let h = face.rect.height as usize;
    let primary_edge = match corner {
        Corner::TopLeft | Corner::TopRight => Edge::Top,
        Corner::BottomLeft | Corner::BottomRight => Edge::Bottom,
    };
    if let Some((name, n_edge, rev)) = neighbor_edge(&face.face, primary_edge) {
        if let Some(neighbor) = group.get(name) {
            return sample_edge(source, &neighbor.rect, n_edge, rev, i, w.max(1), depth);
        }
    }
    let (secondary_edge, secondary_i) = match corner {
        Corner::TopLeft => (Edge::Left, 0),
        Corner::TopRight => (Edge::Right, 0),
        Corner::BottomLeft => (Edge::Left, h.saturating_sub(1) as i32),
        Corner::BottomRight => (Edge::Right, h.saturating_sub(1) as i32),
    };
    if let Some((name, n_edge, rev)) = neighbor_edge(&face.face, secondary_edge) {
        if let Some(neighbor) = group.get(name) {
            return sample_edge(
                source,
                &neighbor.rect,
                n_edge,
                rev,
                secondary_i,
                h.max(1),
                depth,
            );
        }
    }
    // Neither adjacent face exists: sample diagonally outside the face so the
    // corner matches the atlas context plain xBRZ would use.
    let (x, y) = match corner {
        Corner::TopLeft => (
            face.rect.x.saturating_sub(1 + depth),
            face.rect.y.saturating_sub(1 + depth),
        ),
        Corner::TopRight => (
            face.rect.right() + depth,
            face.rect.y.saturating_sub(1 + depth),
        ),
        Corner::BottomLeft => (
            face.rect.x.saturating_sub(1 + depth),
            face.rect.bottom() + depth,
        ),
        Corner::BottomRight => (face.rect.right() + depth, face.rect.bottom() + depth),
    };
    sample_clamped(source, x as i64, y as i64)
}

    /// (w+2·border)×(h+2·border) tile: interior = face's own atlas pixels, the
    /// border ring = each 3D-adjacent face sampled from the shared edge inward
    /// (`depth` walks into that neighbor), or the atlas pixels just outside
    /// this face when the neighbor is absent.
    ///
    /// Border pixels never drop below the alpha of the face edge they frame:
    /// a transparent atlas gap next to an opaque edge would otherwise bleed
    /// holes into the face when xBRZ interpolates (frog head/tongue borders).
    fn build_padded_tile(
        source: &RgbaImage,
        face: &ModelFace,
        group: &HashMap<&str, &ModelFace>,
        border: u32,
    ) -> RgbaImage {
        let (w, h) = (face.rect.width, face.rect.height);
        let b = border.max(1);
        let mut tile = RgbaImage::new(w + 2 * b, h + 2 * b);

        for y in 0..h {
            for x in 0..w {
                let pixel = sample_clamped(
                    source,
                    (face.rect.x + x) as i64,
                    (face.rect.y + y) as i64,
                );
                tile.put_pixel(x + b, y + b, pixel);
            }
        }

        let (w_i, h_i, b_i) = (w as i32, h as i32, b as i32);

        for i in 0..w_i {
            for depth in 0..b {
                let top_y = b - 1 - depth;
                let edge_px = sample_edge(source, &face.rect, Edge::Top, false, i, w as usize, 0);
                let pixel = floor_border_alpha(
                    neighbor_or_wrap(source, face, group, Edge::Top, i, w as usize, depth),
                    edge_px,
                );
                tile.put_pixel((b_i + i) as u32, top_y, pixel);

                let bottom_y = b + h + depth;
                let edge_px =
                    sample_edge(source, &face.rect, Edge::Bottom, false, i, w as usize, 0);
                let pixel = floor_border_alpha(
                    neighbor_or_wrap(source, face, group, Edge::Bottom, i, w as usize, depth),
                    edge_px,
                );
                tile.put_pixel((b_i + i) as u32, bottom_y, pixel);
            }
        }

        for i in 0..h_i {
            for depth in 0..b {
                let left_x = b - 1 - depth;
                let edge_px = sample_edge(source, &face.rect, Edge::Left, false, i, h as usize, 0);
                let pixel = floor_border_alpha(
                    neighbor_or_wrap(source, face, group, Edge::Left, i, h as usize, depth),
                    edge_px,
                );
                tile.put_pixel(left_x, (b_i + i) as u32, pixel);

                let right_x = b + w + depth;
                let edge_px = sample_edge(source, &face.rect, Edge::Right, false, i, h as usize, 0);
                let pixel = floor_border_alpha(
                    neighbor_or_wrap(source, face, group, Edge::Right, i, h as usize, depth),
                    edge_px,
                );
                tile.put_pixel(right_x, (b_i + i) as u32, pixel);
            }
        }

        for cy in 0..b {
            for cx in 0..b {
                let depth_y = b - 1 - cy;
                let cx_i = cx as i32;

                // Top-left: primary along-index extends left of the face (≤ 0).
                let i = cx_i - b_i;
                let edge_px = corner_face_edge(source, face, Corner::TopLeft, i);
                let pixel = floor_border_alpha(
                    corner_pixel(source, face, group, Corner::TopLeft, i, depth_y),
                    edge_px,
                );
                tile.put_pixel(cx, cy, pixel);

                // Top-right: along-index extends right of the face (≥ w).
                let i = w_i + cx_i;
                let edge_px = corner_face_edge(source, face, Corner::TopRight, i);
                let pixel = floor_border_alpha(
                    corner_pixel(source, face, group, Corner::TopRight, i, depth_y),
                    edge_px,
                );
                tile.put_pixel(b + w + cx, cy, pixel);

                // Bottom corners: depth grows downward from the interior.
                let depth = cy;
                let i = cx_i - b_i;
                let edge_px = corner_face_edge(source, face, Corner::BottomLeft, i);
                let pixel = floor_border_alpha(
                    corner_pixel(source, face, group, Corner::BottomLeft, i, depth),
                    edge_px,
                );
                tile.put_pixel(cx, b + h + cy, pixel);

                let i = w_i + cx_i;
                let edge_px = corner_face_edge(source, face, Corner::BottomRight, i);
                let pixel = floor_border_alpha(
                    corner_pixel(source, face, group, Corner::BottomRight, i, depth),
                    edge_px,
                );
                tile.put_pixel(b + w + cx, b + h + cy, pixel);
            }
        }

        tile
    }

    /// Clamp a border sample so its alpha is at least the face-edge alpha it
    /// frames. When the sample is fully transparent (atlas gap), reuse the
    /// face-edge color too so xBRZ does not interpolate toward black.
    fn floor_border_alpha(sample: Rgba<u8>, edge: Rgba<u8>) -> Rgba<u8> {
        if sample.0[3] >= edge.0[3] {
            return sample;
        }
        if sample.0[3] == 0 {
            return edge;
        }
        let mut out = sample;
        out.0[3] = edge.0[3];
        out
    }

    /// Face-edge pixel at along-edge index `i` for a corner's primary edge
    /// (depth 0 into the face). Used as the alpha/color floor for that corner.
    fn corner_face_edge(source: &RgbaImage, face: &ModelFace, corner: Corner, i: i32) -> Rgba<u8> {
        let edge = match corner {
            Corner::TopLeft | Corner::TopRight => Edge::Top,
            Corner::BottomLeft | Corner::BottomRight => Edge::Bottom,
        };
        let len = match edge {
            Edge::Top | Edge::Bottom => face.rect.width.max(1) as usize,
            Edge::Left | Edge::Right => face.rect.height.max(1) as usize,
        };
        sample_edge(source, &face.rect, edge, false, i, len, 0)
    }

    /// Neighbor-bordered cutout of `faces[index]`: interior is the face's atlas
    /// rect, surrounded by a `border`-pixel ring taken from the 3D-adjacent faces
    /// in the same cube (atlas pixels outside the face when a neighbor is missing).
pub fn face_border_tile(
    source: &RgbaImage,
    faces: &[ModelFace],
    index: usize,
    border: u32,
) -> Result<RgbaImage, String> {
    let face = faces
        .get(index)
        .ok_or_else(|| format!("face index {index} out of range"))?;
    if face.rect.width == 0 || face.rect.height == 0 {
        return Err("cannot border a zero-size face".into());
    }
    if border == 0 {
        return Err("border must be at least 1".into());
    }
    let group: HashMap<&str, &ModelFace> = faces
        .iter()
        .filter(|other| other.group == face.group)
        .map(|other| (other.face.as_str(), other))
        .collect();
    Ok(build_padded_tile(source, face, &group, border))
}

/// Empty canvas; each face is pasted as its own neighbor-bordered tile
/// (upscaled, border cropped) at `rect × factor`. Uncovered atlas pixels
/// stay transparent — no plain-xBRZ base under the faces.
fn upscale_box_faces(
    image: &RgbaImage,
    faces: &[ModelFace],
    factor: u32,
) -> Result<RgbaImage, String> {
    let (source_width, source_height) = (image.width(), image.height());
    let (output_width, output_height) = (
        source_width * factor,
        source_height * factor,
    );

    let mut output = RgbaImage::from_pixel(output_width, output_height, Rgba([0, 0, 0, 0]));

    let mut boxes: HashMap<u32, Vec<usize>> = HashMap::new();
    for (index, face) in faces.iter().enumerate() {
        boxes.entry(face.group).or_default().push(index);
    }

    for indices in boxes.values() {
        let mut group: HashMap<&str, &ModelFace> = HashMap::new();
        for &index in indices {
            let face = &faces[index];
            group.entry(face.face.as_str()).or_insert(face);
        }

        for &index in indices {
            let face = &faces[index];
            if face.rect.width == 0
                || face.rect.height == 0
                || face.rect.x >= source_width
                || face.rect.y >= source_height
            {
                continue;
            }

            let padded = build_padded_tile(image, face, &group, 1);
            let upscaled = upscale_image(
                &padded,
                None,
                &UpscaleConfig {
                    factor,
                    stitch_faces: false,
                },
            )?;
            let cropped = image::imageops::crop_imm(
                &upscaled,
                factor,
                factor,
                face.rect.width * factor,
                face.rect.height * factor,
            )
            .to_image();

            for y in 0..cropped.height() {
                let output_y = face.rect.y * factor + y;
                if output_y >= output_height {
                    break;
                }
                for x in 0..cropped.width() {
                    let output_x = face.rect.x * factor + x;
                    if output_x >= output_width {
                        break;
                    }
                    output.put_pixel(output_x, output_y, *cropped.get_pixel(x, y));
                }
            }
        }
    }

    Ok(output)
}

/// Distinct cube/element groups among the faces (used for status messages).
pub fn box_count(faces: &[ModelFace]) -> usize {
    faces.iter().map(|face| face.group).collect::<HashSet<_>>().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use Edge::*;

    fn face(x: u32, y: u32, width: u32, height: u32) -> ModelFace {
        ModelFace {
            texture: String::new(),
            rect: FaceRect {
                x,
                y,
                width,
                height,
            },
            face: "north".into(),
            group: 0,
        }
    }

    /// Box-UV fixture: cube with dx=dy=dz=2, uv(0,0) on an 8×4 image where
    /// pixel = (x, y, 0, 255).
    fn box_fixture() -> (RgbaImage, Vec<ModelFace>) {
        let mut image = RgbaImage::new(8, 4);
        for y in 0..4 {
            for x in 0..8 {
                image.put_pixel(x, y, Rgba([x as u8, y as u8, 0, 255]));
            }
        }
        let rect = |x, y, width, height| FaceRect {
            x,
            y,
            width,
            height,
        };
        let faces = vec![
            ModelFace {
                texture: String::new(),
                rect: rect(2, 0, 2, 2),
                face: "down".into(),
                group: 0,
            },
            ModelFace {
                texture: String::new(),
                rect: rect(4, 0, 2, 2),
                face: "up".into(),
                group: 0,
            },
            ModelFace {
                texture: String::new(),
                rect: rect(4, 2, 2, 2),
                face: "east".into(),
                group: 0,
            },
            ModelFace {
                texture: String::new(),
                rect: rect(2, 2, 2, 2),
                face: "north".into(),
                group: 0,
            },
            ModelFace {
                texture: String::new(),
                rect: rect(0, 2, 2, 2),
                face: "west".into(),
                group: 0,
            },
            ModelFace {
                texture: String::new(),
                rect: rect(6, 2, 2, 2),
                face: "south".into(),
                group: 0,
            },
        ];
        (image, faces)
    }

    fn group_map<'a>(faces: &'a [ModelFace]) -> HashMap<&'a str, &'a ModelFace> {
        faces
            .iter()
            .map(|face| (face.face.as_str(), face))
            .collect()
    }

    #[test]
    fn validates_factor() {
        assert!(UpscaleConfig {
            factor: 1,
            stitch_faces: false
        }
        .validate()
        .is_err());
        assert!(UpscaleConfig {
            factor: 6,
            stitch_faces: true
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn neighbor_table_covers_all_faces() {
        for name in ["down", "up", "west", "north", "east", "south"] {
            for edge in [Top, Bottom, Left, Right] {
                assert!(
                    neighbor_edge(name, edge).is_some(),
                    "missing neighbor for {name}/{edge:?}"
                );
            }
        }
    }

    #[test]
    fn neighbor_table_is_symmetric() {
        for name in ["down", "up", "west", "north", "east", "south"] {
            for edge in [Top, Bottom, Left, Right] {
                let (neighbor, n_edge, rev) = neighbor_edge(name, edge).unwrap();
                let (back, back_edge, back_rev) = neighbor_edge(neighbor, n_edge).unwrap();
                assert_eq!((back, back_edge, back_rev), (name, edge, rev));
            }
        }
    }

    #[test]
    fn builds_tile_with_neighbor_strips() {
        let (image, faces) = box_fixture();
        let group = group_map(&faces);
        let find = |name: &str| faces.iter().find(|face| face.face == name).unwrap();

        // west.Top comes from down's left column (atlas row 0): src(2,0),(2,1).
        let west = build_padded_tile(&image, find("west"), &group, 1);
        assert_eq!(west.dimensions(), (4, 4));
        assert_eq!(*west.get_pixel(1, 0), Rgba([2, 0, 0, 255]));
        assert_eq!(*west.get_pixel(2, 0), Rgba([2, 1, 0, 255]));

        // south.Right comes from west's left column (far across the atlas):
        // src(0,2),(0,3) — the seam the old bbox grouping missed.
        let south = build_padded_tile(&image, find("south"), &group, 1);
        assert_eq!(*south.get_pixel(3, 1), Rgba([0, 2, 0, 255]));
        assert_eq!(*south.get_pixel(3, 2), Rgba([0, 3, 0, 255]));

        // down.Right comes from east's top row, reversed: src(5,2),src(4,2).
        let down = build_padded_tile(&image, find("down"), &group, 1);
        assert_eq!(*down.get_pixel(3, 1), Rgba([5, 2, 0, 255]));
        assert_eq!(*down.get_pixel(3, 2), Rgba([4, 2, 0, 255]));

        // Corner: west's top-left corner uses down's left column start.
        assert_eq!(*west.get_pixel(0, 0), Rgba([2, 0, 0, 255]));
    }

    #[test]
    fn lone_face_uses_atlas_outside_own_edges() {
        let (image, faces) = box_fixture();
        let north = faces
            .iter()
            .find(|face| face.face == "north")
            .unwrap()
            .clone();
        let group: HashMap<&str, &ModelFace> = [("north", &north)].into_iter().collect();
        // north.Top samples the atlas row above the face (y = rect.y - 1).
        let tile = build_padded_tile(&image, &north, &group, 1);
        let above_y = north.rect.y.saturating_sub(1);
        for i in 0..north.rect.width {
            let expected = *image.get_pixel(north.rect.x + i, above_y);
            assert_eq!(
                *tile.get_pixel(i + 1, 0),
                expected,
                "top border i={i} should be atlas above face"
            );
        }
        // north.Bottom samples the atlas row below the face.
        let below_y = north.rect.bottom();
        for i in 0..north.rect.width {
            let expected = *image.get_pixel(north.rect.x + i, below_y.min(image.height() - 1));
            assert_eq!(
                *tile.get_pixel(i + 1, tile.height() - 1),
                expected,
                "bottom border i={i} should be atlas below face"
            );
        }
    }

    #[test]
    fn upscales_dimensions() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
        let config = UpscaleConfig {
            factor: 4,
            stitch_faces: false,
        };
        let output = upscale_image(&image, None, &config).unwrap();
        assert_eq!(output.dimensions(), (16, 16));
    }

    #[test]
    fn wrap_tile_fills_border_from_opposite_edges_and_corners() {
        // Encode x * 10 + y in the red channel of a 3x2 texture.
        let mut image = RgbaImage::new(3, 2);
        for y in 0..2 {
            for x in 0..3 {
                image.put_pixel(x, y, Rgba([x as u8 * 10 + y as u8, 0, 0, 255]));
            }
        }
        let padded = wrap_tile(&image);
        assert_eq!(padded.dimensions(), (5, 4));
        let red = |x: u32, y: u32| padded.get_pixel(x, y).0[0];
        // Interior passthrough: padded(1,1) = src(0,0).
        assert_eq!(red(1, 1), 0);
        // Top edge gets the bottom row: padded(2,0) = src(1,1).
        assert_eq!(red(2, 0), 11);
        // Bottom edge gets the top row: padded(2,3) = src(1,0).
        assert_eq!(red(2, 3), 10);
        // Left edge gets the right column: padded(0,1) = src(2,0).
        assert_eq!(red(0, 1), 20);
        // Right edge gets the left column: padded(4,1) = src(0,0).
        assert_eq!(red(4, 1), 0);
        // Corners: opposite corner of the texture.
        assert_eq!(red(0, 0), 21); // = src(2,1)
        assert_eq!(red(4, 0), 1); // = src(0,1)
        assert_eq!(red(0, 3), 20); // = src(2,0)
        assert_eq!(red(4, 3), 0); // = src(0,0)
    }

    #[test]
    fn upscale_wrapped_matches_plain_dimensions() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
        let output = upscale_wrapped(&image, 4).unwrap();
        assert_eq!(output.dimensions(), (16, 16));

        let single = RgbaImage::from_pixel(1, 1, Rgba([1, 2, 3, 4]));
        let output = upscale_wrapped(&single, 2).unwrap();
        assert_eq!(output.dimensions(), (2, 2));
        assert_eq!(*output.get_pixel(0, 0), Rgba([1, 2, 3, 4]));
    }

    #[test]
    fn box_faces_stitch_dims_and_border_context() {
        let (image, faces) = box_fixture();
        let config = UpscaleConfig {
            factor: 4,
            stitch_faces: true,
        };
        let stitched = upscale_image(&image, Some(&faces), &config).unwrap();
        assert_eq!(stitched.dimensions(), (32, 16));

        let plain = upscale_image(
            &image,
            None,
            &UpscaleConfig {
                factor: 4,
                stitch_faces: false,
            },
        )
        .unwrap();
        // down occupies dest x8..15, y0..7 — its border rows/columns must use
        // neighbor context, so the stitched output differs from plain xBRZ
        // somewhere inside that box.
        let mut differs = false;
        for y in 0..8 {
            for x in 8..16 {
                if stitched.get_pixel(x, y).0 != plain.get_pixel(x, y).0 {
                    differs = true;
                }
            }
        }
        assert!(differs, "stitched face border should use neighbor context");

        assert_eq!(box_count(&faces), 1);
        let mut second = face(0, 4, 2, 2);
        second.group = 1;
        assert_eq!(box_count(&[face(0, 0, 2, 2), second]), 2);

        // Faces paste onto an empty canvas: a pixel outside every face rect
        // must stay transparent (no plain-xBRZ base underneath).
        // west is (0,2,2×2) → dest (0,8)-(8,16); (0,0) is outside all faces.
        assert_eq!(stitched.get_pixel(0, 0).0[3], 0, "non-face pixel stays empty");
    }

    #[test]
    fn border_alpha_never_drops_below_face_edge() {
        // Opaque face edge framed by a transparent neighbor/atlas gap: the
        // border ring must not go transparent or xBRZ bleeds holes into the
        // face (frog head/tongue).
        let mut image = RgbaImage::new(4, 4);
        // Left 2×2 opaque green, right 2×2 transparent — two faces sharing
        // the vertical seam at x=2.
        for y in 0..4 {
            for x in 0..2 {
                image.put_pixel(x, y, Rgba([10, 200, 30, 255]));
            }
            for x in 2..4 {
                image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        // west = opaque column strip, east = transparent neighbor in same cube.
        let west = ModelFace {
            texture: String::new(),
            rect: FaceRect {
                x: 0,
                y: 0,
                width: 2,
                height: 4,
            },
            face: "west".into(),
            group: 0,
        };
        let east = ModelFace {
            texture: String::new(),
            rect: FaceRect {
                x: 2,
                y: 0,
                width: 2,
                height: 4,
            },
            face: "east".into(),
            group: 0,
        };
        let group: HashMap<&str, &ModelFace> =
            [("west", &west), ("east", &east)].into_iter().collect();
        let tile = build_padded_tile(&image, &west, &group, 1);
        // Right border column (x = 1 + 2 = 3) sits against the transparent
        // east face; every sample there must stay fully opaque (west.Right
        // edge pixels are opaque).
        for y in 0..tile.height() {
            let a = tile.get_pixel(tile.width() - 1, y).0[3];
            assert_eq!(a, 255, "right border y={y} alpha={a}");
        }
    }

    #[test]
    fn missing_neighbor_border_uses_atlas_outside_not_opposite_edge() {
        // Sparse face: opposite edge is transparent, atlas above is opaque
        // pink (axolotl gill). Missing top neighbor must sample outside, not
        // wrap the bottom edge.
        let mut image = RgbaImage::new(6, 6);
        for x in 0..6 {
            image.put_pixel(x, 1, Rgba([220, 80, 140, 255])); // atlas row above face
        }
        // Face at (1,2) 4×2 — interior transparent except one opaque pixel.
        image.put_pixel(2, 3, Rgba([220, 80, 140, 255]));
        let north = ModelFace {
            texture: String::new(),
            rect: FaceRect {
                x: 1,
                y: 2,
                width: 4,
                height: 2,
            },
            face: "north".into(),
            group: 0,
        };
        let group: HashMap<&str, &ModelFace> = [("north", &north)].into_iter().collect();
        let tile = build_padded_tile(&image, &north, &group, 1);
        // Top border (y=0) must be the atlas row above (opaque pink).
        for x in 0..4 {
            let px = tile.get_pixel(x + 1, 0);
            assert_eq!(px.0[3], 255, "top border x={x} should use atlas outside");
            assert_eq!(px.0[..3], [220, 80, 140], "top border color");
        }
        // Bottom border under transparent face-edge columns keeps atlas below
        // (row 4 is transparent). Column under the opaque face pixel is
        // floored opaque by floor_border_alpha (must not punch a hole).
        for x in 0..4 {
            let face_edge_opaque = x == 1; // face pixel (2,3) maps to border i=1
            let a = tile.get_pixel(x + 1, tile.height() - 1).0[3];
            if face_edge_opaque {
                assert_eq!(a, 255, "bottom border under opaque edge stays opaque");
            } else {
                assert_eq!(a, 0, "bottom border x={x} uses transparent atlas below");
            }
        }
    }

}
