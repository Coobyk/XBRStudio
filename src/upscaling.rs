use std::collections::HashMap;

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

    let mut output = RgbaImage::from_pixel(output_width, output_height, Rgba([0, 0, 0, 0]));

    let base = scale_rgba(
        image.as_raw(),
        source_width as usize,
        source_height as usize,
        config.factor as usize,
    );
    copy_rgba_buffer(&mut output, &base);

    let groups = group_stitch_faces(faces, source_width, source_height);
    for group in groups {
        let group_faces: Vec<&ModelFace> = group.iter().map(|&index| &faces[index]).collect();
        let bounds = match bounds_of_faces(&group_faces) {
            Some(bounds) => bounds,
            None => continue,
        };

        upscale_group_into(image, &mut output, &group_faces, &bounds, config.factor)?;
    }

    Ok(output)
}

fn copy_rgba_buffer(output: &mut RgbaImage, rgba: &[u8]) {
    let target = output.as_mut();
    let len = target.len().min(rgba.len());
    target[..len].copy_from_slice(&rgba[..len]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegionBounds {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn bounds_of_faces(faces: &[&ModelFace]) -> Option<RegionBounds> {
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = u32::MIN;
    let mut max_y = u32::MIN;

    for model_face in faces {
        let rect = &model_face.rect;
        min_x = min_x.min(rect.x);
        min_y = min_y.min(rect.y);
        max_x = max_x.max(rect.right());
        max_y = max_y.max(rect.bottom());
    }

    if min_x >= max_x || min_y >= max_y {
        return None;
    }

    Some(RegionBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn upscale_group_into(
    source: &RgbaImage,
    output: &mut RgbaImage,
    group: &[&ModelFace],
    bounds: &RegionBounds,
    factor: u32,
) -> Result<(), String> {
    if bounds.width == 0 || bounds.height == 0 {
        return Ok(());
    }

    let mut region = RgbaImage::new(bounds.width, bounds.height);
    for y in 0..bounds.height {
        for x in 0..bounds.width {
            let source_x = bounds.x + x;
            let source_y = bounds.y + y;
            if source_x < source.width() && source_y < source.height() {
                region.put_pixel(x, y, *source.get_pixel(source_x, source_y));
            }
        }
    }

    let scaled_width = bounds.width * factor;
    let scaled_height = bounds.height * factor;
    let scaled = scale_rgba(
        region.as_raw(),
        bounds.width as usize,
        bounds.height as usize,
        factor as usize,
    );
    let scaled_image = RgbaImage::from_raw(scaled_width, scaled_height, scaled)
        .ok_or_else(|| "xBRZ produced an invalid group buffer".to_string())?;

    let member_rects: Vec<FaceRect> = group.iter().map(|face| face.rect).collect();

    for member in &member_rects {
        let local_x_start = (member.x - bounds.x) * factor;
        let local_y_start = (member.y - bounds.y) * factor;
        let width = member.width * factor;
        let height = member.height * factor;

        for y in 0..height {
            for x in 0..width {
                let scaled_pixel = *scaled_image.get_pixel(local_x_start + x, local_y_start + y);
                let output_x = member.x * factor + x;
                let output_y = member.y * factor + y;
                if output_x < output.width() && output_y < output.height() {
                    output.put_pixel(output_x, output_y, scaled_pixel);
                }
            }
        }
    }

    Ok(())
}

pub fn group_stitch_faces(
    faces: &[ModelFace],
    atlas_width: u32,
    atlas_height: u32,
) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..faces.len()).collect();

    for (index, face) in faces.iter().enumerate() {
        for other_index in (index + 1)..faces.len() {
            let other = &faces[other_index];
            if within_atlas(&face.rect, atlas_width, atlas_height)
                && within_atlas(&other.rect, atlas_width, atlas_height)
                && rects_share_edge(&face.rect, &other.rect)
            {
                union(&mut parent, index, other_index);
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..faces.len() {
        let root = find(&mut parent, index);
        groups.entry(root).or_default().push(index);
    }

    groups.into_values().collect()
}

fn within_atlas(rect: &FaceRect, width: u32, height: u32) -> bool {
    rect.x < width
        && rect.y < height
        && rect.right() <= width
        && rect.bottom() <= height
}

fn rects_share_edge(a: &FaceRect, b: &FaceRect) -> bool {
    if a.width == 0 || a.height == 0 || b.width == 0 || b.height == 0 {
        return false;
    }

    let horizontal_neighbors = (a.right() == b.x || b.right() == a.x)
        && intervals_overlap(a.y, a.bottom(), b.y, b.bottom());
    let vertical_neighbors = (a.bottom() == b.y || b.bottom() == a.y)
        && intervals_overlap(a.x, a.right(), b.x, b.right());

    horizontal_neighbors || vertical_neighbors
}

fn intervals_overlap(a_start: u32, a_end: u32, b_start: u32, b_end: u32) -> bool {
    a_start < b_end && b_start < a_end
}

fn find(parent: &mut [usize], node: usize) -> usize {
    let mut root = node;
    while parent[root] != root {
        root = parent[root];
    }

    let mut current = node;
    while parent[current] != root {
        let next = parent[current];
        parent[current] = root;
        current = next;
    }

    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let root_a = find(parent, a);
    let root_b = find(parent, b);
    if root_a != root_b {
        parent[root_b] = root_a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
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
    fn detects_edge_neighbors() {
        assert!(rects_share_edge(
            &FaceRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8
            },
            &FaceRect {
                x: 8,
                y: 0,
                width: 8,
                height: 8
            },
        ));
        assert!(!rects_share_edge(
            &FaceRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8
            },
            &FaceRect {
                x: 8,
                y: 8,
                width: 8,
                height: 8
            },
        ));
    }

    #[test]
    fn groups_connected_faces() {
        let faces = vec![face(0, 0, 8, 8), face(8, 0, 8, 8), face(0, 16, 8, 8)];

        let groups = group_stitch_faces(&faces, 16, 24);
        let mut sizes: Vec<usize> = groups.iter().map(Vec::len).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![1, 2]);
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
    fn stitched_neighbors_share_context() {
        let mut image = RgbaImage::new(16, 8);
        for y in 0..8 {
            for x in 0..8 {
                image.put_pixel(x, y, Rgba([220, 40, 40, 255]));
            }
            for x in 8..16 {
                image.put_pixel(x, y, Rgba([40, 40, 220, 255]));
            }
        }

        let faces = vec![face(0, 0, 8, 8), face(8, 0, 8, 8)];
        let config = UpscaleConfig {
            factor: 4,
            stitch_faces: true,
        };
        let stitched = upscale_image(&image, Some(&faces), &config).unwrap();

        let mut independent = RgbaImage::from_pixel(64, 32, Rgba([0, 0, 0, 0]));
        for model_face in &faces {
            let rect = model_face.rect;
            let mut region = RgbaImage::new(rect.width, rect.height);
            for y in 0..rect.height {
                for x in 0..rect.width {
                    region.put_pixel(x, y, *image.get_pixel(rect.x + x, rect.y + y));
                }
            }
            let scaled = scale_rgba(
                region.as_raw(),
                rect.width as usize,
                rect.height as usize,
                config.factor as usize,
            );
            let scaled_image = RgbaImage::from_raw(
                rect.width * config.factor,
                rect.height * config.factor,
                scaled,
            )
            .unwrap();
            for y in 0..scaled_image.height() {
                for x in 0..scaled_image.width() {
                    independent.put_pixel(
                        rect.x * config.factor + x,
                        rect.y * config.factor + y,
                        *scaled_image.get_pixel(x, y),
                    );
                }
            }
        }

        assert_eq!(stitched.dimensions(), (64, 32));
        let mut differs_at_seam = false;
        for y in 0..stitched.height() {
            if stitched.get_pixel(31, y).0 != independent.get_pixel(31, y).0
                || stitched.get_pixel(32, y).0 != independent.get_pixel(32, y).0
            {
                differs_at_seam = true;
                break;
            }
        }
        assert!(
            differs_at_seam,
            "stitched seam should use combined face context"
        );
    }
}
