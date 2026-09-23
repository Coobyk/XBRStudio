use std::collections::{HashMap, HashSet};

use image::{Rgba, RgbaImage};
use rayon::prelude::*;
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
    let (output_width, output_height) =
        (source_width * config.factor, source_height * config.factor);

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
        Edge::Bottom => (rect.x + p as u32, rect.bottom().saturating_sub(1 + d_y)),
        Edge::Left => (rect.x + d_x, rect.y + p as u32),
        Edge::Right => (rect.right().saturating_sub(1 + d_x), rect.y + p as u32),
    };
    sample_clamped(image, x as i64, y as i64)
}

/// 3D-adjacent face sample for this edge, or `None` when the neighbor face is
/// absent (flat cubes: axolotl tail/legs/gills) — those borders stay empty.
fn neighbor_or_wrap(
    source: &RgbaImage,
    face: &ModelFace,
    group: &HashMap<&str, &ModelFace>,
    edge: Edge,
    i: i32,
    len: usize,
    depth: u32,
) -> Option<Rgba<u8>> {
    let (name, n_edge, rev) = neighbor_edge(&face.face, edge)?;
    let neighbor = group.get(name)?;
    Some(sample_edge(
        source,
        &neighbor.rect,
        n_edge,
        rev,
        i,
        len,
        depth,
    ))
}

#[derive(Clone, Copy)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Corner sample from the neighbor across the primary edge, else the
/// secondary-edge neighbor, else `None` (corner stays empty).
fn corner_pixel(
    source: &RgbaImage,
    face: &ModelFace,
    group: &HashMap<&str, &ModelFace>,
    corner: Corner,
    i: i32,
    depth: u32,
) -> Option<Rgba<u8>> {
    let w = face.rect.width as usize;
    let h = face.rect.height as usize;
    let primary_edge = match corner {
        Corner::TopLeft | Corner::TopRight => Edge::Top,
        Corner::BottomLeft | Corner::BottomRight => Edge::Bottom,
    };
    if let Some((name, n_edge, rev)) = neighbor_edge(&face.face, primary_edge) {
        if let Some(neighbor) = group.get(name) {
            return Some(sample_edge(
                source,
                &neighbor.rect,
                n_edge,
                rev,
                i,
                w.max(1),
                depth,
            ));
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
            return Some(sample_edge(
                source,
                &neighbor.rect,
                n_edge,
                rev,
                secondary_i,
                h.max(1),
                depth,
            ));
        }
    }
    None
}

/// (w+2·border)×(h+2·border) tile: interior = face's own atlas pixels, the
/// border ring = each 3D-adjacent face sampled from the shared edge inward
/// (`depth` walks into that neighbor). Missing neighbors stay transparent —
/// no atlas-outside fill, no opposite-edge wrap.
///
/// When a neighbor sample is present, its alpha never drops below the face
/// edge it frames: a transparent atlas gap next to an opaque edge would
/// otherwise bleed holes into the face when xBRZ interpolates (frog).
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
            let pixel = sample_clamped(source, (face.rect.x + x) as i64, (face.rect.y + y) as i64);
            tile.put_pixel(x + b, y + b, pixel);
        }
    }

    let (w_i, h_i, b_i) = (w as i32, h as i32, b as i32);

    for i in 0..w_i {
        for depth in 0..b {
            let top_y = b - 1 - depth;
            let pixel = match neighbor_or_wrap(source, face, group, Edge::Top, i, w as usize, depth)
            {
                Some(sample) => {
                    let edge_px =
                        sample_edge(source, &face.rect, Edge::Top, false, i, w as usize, 0);
                    floor_border_alpha(sample, edge_px)
                }
                None => Rgba([0, 0, 0, 0]),
            };
            tile.put_pixel((b_i + i) as u32, top_y, pixel);

            let bottom_y = b + h + depth;
            let pixel =
                match neighbor_or_wrap(source, face, group, Edge::Bottom, i, w as usize, depth) {
                    Some(sample) => {
                        let edge_px =
                            sample_edge(source, &face.rect, Edge::Bottom, false, i, w as usize, 0);
                        floor_border_alpha(sample, edge_px)
                    }
                    None => Rgba([0, 0, 0, 0]),
                };
            tile.put_pixel((b_i + i) as u32, bottom_y, pixel);
        }
    }

    for i in 0..h_i {
        for depth in 0..b {
            let left_x = b - 1 - depth;
            let pixel =
                match neighbor_or_wrap(source, face, group, Edge::Left, i, h as usize, depth) {
                    Some(sample) => {
                        let edge_px =
                            sample_edge(source, &face.rect, Edge::Left, false, i, h as usize, 0);
                        floor_border_alpha(sample, edge_px)
                    }
                    None => Rgba([0, 0, 0, 0]),
                };
            tile.put_pixel(left_x, (b_i + i) as u32, pixel);

            let right_x = b + w + depth;
            let pixel =
                match neighbor_or_wrap(source, face, group, Edge::Right, i, h as usize, depth) {
                    Some(sample) => {
                        let edge_px =
                            sample_edge(source, &face.rect, Edge::Right, false, i, h as usize, 0);
                        floor_border_alpha(sample, edge_px)
                    }
                    None => Rgba([0, 0, 0, 0]),
                };
            tile.put_pixel(right_x, (b_i + i) as u32, pixel);
        }
    }

    for cy in 0..b {
        for cx in 0..b {
            let depth_y = b - 1 - cy;
            let cx_i = cx as i32;

            // Top-left: primary along-index extends left of the face (≤ 0).
            let i = cx_i - b_i;
            let pixel = match corner_pixel(source, face, group, Corner::TopLeft, i, depth_y) {
                Some(sample) => {
                    let edge_px = corner_face_edge(source, face, Corner::TopLeft, i);
                    floor_border_alpha(sample, edge_px)
                }
                None => Rgba([0, 0, 0, 0]),
            };
            tile.put_pixel(cx, cy, pixel);

            // Top-right: along-index extends right of the face (≥ w).
            let i = w_i + cx_i;
            let pixel = match corner_pixel(source, face, group, Corner::TopRight, i, depth_y) {
                Some(sample) => {
                    let edge_px = corner_face_edge(source, face, Corner::TopRight, i);
                    floor_border_alpha(sample, edge_px)
                }
                None => Rgba([0, 0, 0, 0]),
            };
            tile.put_pixel(b + w + cx, cy, pixel);

            // Bottom corners: depth grows downward from the interior.
            let depth = cy;
            let i = cx_i - b_i;
            let pixel = match corner_pixel(source, face, group, Corner::BottomLeft, i, depth) {
                Some(sample) => {
                    let edge_px = corner_face_edge(source, face, Corner::BottomLeft, i);
                    floor_border_alpha(sample, edge_px)
                }
                None => Rgba([0, 0, 0, 0]),
            };
            tile.put_pixel(cx, b + h + cy, pixel);

            let i = w_i + cx_i;
            let pixel = match corner_pixel(source, face, group, Corner::BottomRight, i, depth) {
                Some(sample) => {
                    let edge_px = corner_face_edge(source, face, Corner::BottomRight, i);
                    floor_border_alpha(sample, edge_px)
                }
                None => Rgba([0, 0, 0, 0]),
            };
            tile.put_pixel(b + w + cx, b + h + cy, pixel);
        }
    }

    tile
}

/// Clamp a neighbor border sample so its alpha is at least the face-edge
/// alpha it frames. Fully transparent neighbor gaps reuse the edge color
/// so xBRZ does not interpolate toward black. Missing neighbors are never
/// passed through here — they stay empty.
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
/// in the same cube (transparent when a neighbor is missing).
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

/// Plain-xBRZ base; each face is pasted as its own neighbor-bordered tile
/// (upscaled, border cropped) at `rect × factor`. Texture outside the model's
/// UV rects keeps the plain upscale so partial-UV models (lantern) don't
/// punch holes in unreferenced atlas content.
fn upscale_box_faces(
    image: &RgbaImage,
    faces: &[ModelFace],
    factor: u32,
) -> Result<RgbaImage, String> {
    let (source_width, source_height) = (image.width(), image.height());
    let (output_width, output_height) = (source_width * factor, source_height * factor);

    let mut output = upscale_image(
        image,
        None,
        &UpscaleConfig {
            factor,
            stitch_faces: false,
        },
    )?;

    let mut boxes: HashMap<u32, Vec<usize>> = HashMap::new();
    for (index, face) in faces.iter().enumerate() {
        boxes.entry(face.group).or_default().push(index);
    }

    let mut group_maps: HashMap<u32, HashMap<&str, &ModelFace>> = HashMap::new();
    for (&group_id, indices) in &boxes {
        let mut group: HashMap<&str, &ModelFace> = HashMap::new();
        for &index in indices {
            let face = &faces[index];
            group.entry(face.face.as_str()).or_insert(face);
        }
        group_maps.insert(group_id, group);
    }

    let work: Vec<usize> = faces
        .iter()
        .enumerate()
        .filter(|(_, face)| {
            face.rect.width > 0
                && face.rect.height > 0
                && face.rect.x < source_width
                && face.rect.y < source_height
        })
        .map(|(index, _)| index)
        .collect();

    let upscaled_faces: Result<Vec<(usize, RgbaImage)>, String> = work
        .into_par_iter()
        .map(|index| {
            let face = &faces[index];
            let group = group_maps
                .get(&face.group)
                .ok_or_else(|| "missing face group".to_string())?;
            let padded = build_padded_tile(image, face, group, 1);
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
            Ok((index, cropped))
        })
        .collect();

    let mut upscaled_faces = upscaled_faces?;
    upscaled_faces.sort_by_key(|(index, _)| *index);
    for (index, cropped) in upscaled_faces {
        let face = &faces[index];
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

    Ok(output)
}

/// Distinct cube/element groups among the faces (used for status messages).
pub fn box_count(faces: &[ModelFace]) -> usize {
    faces
        .iter()
        .map(|face| face.group)
        .collect::<HashSet<_>>()
        .len()
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
        assert!(
            UpscaleConfig {
                factor: 1,
                stitch_faces: false
            }
            .validate()
            .is_err()
        );
        assert!(
            UpscaleConfig {
                factor: 6,
                stitch_faces: true
            }
            .validate()
            .is_ok()
        );
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
    fn lone_face_missing_neighbors_stay_empty() {
        let (image, faces) = box_fixture();
        let north = faces
            .iter()
            .find(|face| face.face == "north")
            .unwrap()
            .clone();
        let group: HashMap<&str, &ModelFace> = [("north", &north)].into_iter().collect();
        let tile = build_padded_tile(&image, &north, &group, 1);
        // Top/bottom strips have no 3D neighbor in this sparse group.
        for i in 0..north.rect.width {
            assert_eq!(
                tile.get_pixel(i + 1, 0).0[3],
                0,
                "top border i={i} stays empty"
            );
            assert_eq!(
                tile.get_pixel(i + 1, tile.height() - 1).0[3],
                0,
                "bottom border i={i} stays empty"
            );
        }
        // Left/right strips likewise have no neighbor.
        for i in 0..north.rect.height {
            assert_eq!(tile.get_pixel(0, i + 1).0[3], 0, "left border i={i}");
            assert_eq!(
                tile.get_pixel(tile.width() - 1, i + 1).0[3],
                0,
                "right border i={i}"
            );
        }
        // Interior still samples the face itself.
        assert_eq!(
            tile.get_pixel(1, 1).0[3],
            image.get_pixel(north.rect.x, north.rect.y).0[3]
        );
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

        // Non-face pixels keep the plain-xBRZ base (partial-UV models like the
        // lantern must not punch holes in unreferenced atlas content).
        // west is (0,2,2×2) → dest (0,8)-(8,16); (0,0) is outside all faces
        // but the source pixel there is opaque, so the base remains opaque.
        assert_eq!(
            stitched.get_pixel(0, 0).0[3],
            255,
            "non-face pixel keeps plain-xBRZ base"
        );
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
        // west = opaque column strip; north is west.Right's 3D neighbor and
        // is transparent in the atlas (frog-style gap next to an opaque edge).
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
        let north = ModelFace {
            texture: String::new(),
            rect: FaceRect {
                x: 2,
                y: 0,
                width: 2,
                height: 4,
            },
            face: "north".into(),
            group: 0,
        };
        let group: HashMap<&str, &ModelFace> =
            [("west", &west), ("north", &north)].into_iter().collect();
        let tile = build_padded_tile(&image, &west, &group, 1);
        // Right border column (x = 1 + 2 = 3) sits against the transparent
        // north face; edge samples there must stay fully opaque (west.Right
        // edge pixels are opaque). Corners (y=0 / y=height-1) have no
        // up/down neighbor in this sparse group and stay empty.
        for y in 1..tile.height() - 1 {
            let a = tile.get_pixel(tile.width() - 1, y).0[3];
            assert_eq!(a, 255, "right border y={y} alpha={a}");
        }
    }

    #[test]
    fn missing_neighbor_border_stays_empty_not_atlas_outside() {
        // Sparse face: top neighbor absent. The atlas row above is opaque
        // pink, but the border ring must stay transparent (no atlas-outside
        // fill, no opposite-edge wrap).
        let mut image = RgbaImage::new(6, 6);
        for x in 0..6 {
            image.put_pixel(x, 1, Rgba([220, 80, 140, 255])); // atlas row above face
        }
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
        // Top border must stay empty even though the atlas row above is pink.
        for x in 0..4 {
            assert_eq!(tile.get_pixel(x + 1, 0).0[3], 0, "top border x={x} empty");
        }
        // Bottom border likewise has no neighbor — empty, not floored opaque.
        for x in 0..4 {
            assert_eq!(
                tile.get_pixel(x + 1, tile.height() - 1).0[3],
                0,
                "bottom border x={x} empty"
            );
        }
    }
}
