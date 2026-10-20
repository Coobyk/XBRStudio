//! Convert a legacy 64×32 skin into the modern layout.
//!
//! Old skins only stored the front of every box; the game derived the other
//! faces from it. Modern skins store every face, so a conversion has to
//! place the old pixels into the UV layout of an entity model and fill the
//! remaining faces the way the game used to derive them.

use image::{Rgba, RgbaImage};

use crate::model::{FaceRect, ModelFaces};

/// A region of the legacy skin.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The parts of a legacy 64×32 skin.
#[derive(Debug, Clone, Copy)]
pub struct LegacyParts {
    pub head: Region,
    pub hat: Region,
    pub head_overlay: Region,
    pub hat_overlay: Region,
    pub body: Region,
    pub body_overlay: Region,
    pub right_arm: Region,
    pub right_arm_overlay: Region,
    pub right_leg: Region,
    pub right_leg_overlay: Region,
}

impl LegacyParts {
    /// The classic 64×32 layout.
    pub fn classic() -> LegacyParts {
        LegacyParts {
            head: Region {
                x: 8,
                y: 0,
                width: 8,
                height: 8,
            },
            hat: Region {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            head_overlay: Region {
                x: 8,
                y: 8,
                width: 8,
                height: 8,
            },
            hat_overlay: Region {
                x: 0,
                y: 8,
                width: 8,
                height: 8,
            },
            body: Region {
                x: 16,
                y: 16,
                width: 8,
                height: 12,
            },
            body_overlay: Region {
                x: 16,
                y: 28,
                width: 8,
                height: 4,
            },
            right_arm: Region {
                x: 40,
                y: 16,
                width: 4,
                height: 12,
            },
            right_arm_overlay: Region {
                x: 44,
                y: 16,
                width: 4,
                height: 12,
            },
            right_leg: Region {
                x: 0,
                y: 16,
                width: 4,
                height: 12,
            },
            right_leg_overlay: Region {
                x: 4,
                y: 16,
                width: 4,
                height: 12,
            },
        }
    }
}

/// The part of the model a legacy region belongs to.
fn part_of(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    if name.contains("hat") || name.contains("helmet") {
        return Some("hat");
    }
    if name.contains("head") {
        return Some("head");
    }
    if name.contains("body") || name.contains("torso") {
        return Some("body");
    }
    if name.contains("right_arm") || name.contains("rightarm") {
        return Some("right_arm");
    }
    if name.contains("left_arm") || name.contains("leftarm") {
        return Some("left_arm");
    }
    if name.contains("right_leg") || name.contains("rightleg") {
        return Some("right_leg");
    }
    if name.contains("left_leg") || name.contains("leftleg") {
        return Some("left_leg");
    }
    None
}

/// The legacy region of a model part.
fn legacy_region(parts: &LegacyParts, part: &str, overlay: bool) -> Option<Region> {
    let region = match part {
        "head" => {
            if overlay {
                parts.head_overlay
            } else {
                parts.head
            }
        }
        "hat" => {
            if overlay {
                parts.hat_overlay
            } else {
                parts.hat
            }
        }
        "body" => {
            if overlay {
                parts.body_overlay
            } else {
                parts.body
            }
        }
        "right_arm" => {
            if overlay {
                parts.right_arm_overlay
            } else {
                parts.right_arm
            }
        }
        "right_leg" => {
            if overlay {
                parts.right_leg_overlay
            } else {
                parts.right_leg
            }
        }
        // The left side of a legacy skin is the mirror of the right one.
        "left_arm" => {
            let base = if overlay {
                parts.right_arm_overlay
            } else {
                parts.right_arm
            };
            return Some(Region {
                x: base.x,
                y: base.y,
                width: base.width,
                height: base.height,
            });
        }
        "left_leg" => {
            let base = if overlay {
                parts.right_leg_overlay
            } else {
                parts.right_leg
            };
            return Some(Region {
                x: base.x,
                y: base.y,
                width: base.width,
                height: base.height,
            });
        }
        _ => return None,
    };
    Some(region)
}

/// Convert a legacy skin into the layout of an entity model.
///
/// The model decides where the faces go; the legacy skin decides what they
/// look like. Faces the legacy skin did not store are derived from the front
/// the way the game used to do it.
pub fn convert(legacy: &RgbaImage, model: &ModelFaces) -> Result<RgbaImage, String> {
    if legacy.width() != 64 {
        return Err(format!(
            "the legacy skin must be 64 pixels wide, got {}",
            legacy.width()
        ));
    }
    if legacy.height() < 32 {
        return Err(format!(
            "the legacy skin must be at least 32 pixels high, got {}",
            legacy.height()
        ));
    }

    let parts = LegacyParts::classic();
    // Models without a texture size use the 16×16 block space; a skin is
    // always 64×64.
    let (uv_width, uv_height) = if model.uv_size.0 > 16.0 {
        (
            model.uv_size.0.round() as u32,
            model.uv_size.1.round() as u32,
        )
    } else {
        (64, 64)
    };
    let mut output = RgbaImage::new(uv_width, uv_height);

    // Group the faces of the model by the part they belong to.
    let mut groups: std::collections::BTreeMap<u32, Vec<&crate::model::ModelFace>> =
        std::collections::BTreeMap::new();
    for face in &model.faces {
        groups.entry(face.group).or_default().push(face);
    }

    for group in groups.values() {
        let Some(part) = group
            .iter()
            .find_map(|face| face.part.as_deref())
            .and_then(part_of)
        else {
            continue;
        };
        let Some(region) = legacy_region(&parts, part, false) else {
            continue;
        };
        let source = crop(legacy, region);

        write_box(
            &mut output,
            group,
            &source,
            part == "left_arm" || part == "left_leg",
        );
    }

    Ok(output)
}

/// Cut a region out of an image.
fn crop(image: &RgbaImage, region: Region) -> RgbaImage {
    let width = region.width.min(image.width().saturating_sub(region.x));
    let height = region.height.min(image.height().saturating_sub(region.y));
    image::imageops::crop_imm(image, region.x, region.y, width, height).to_image()
}

/// Write the faces of one box, deriving the ones the legacy skin did not
/// have.
fn write_box(
    output: &mut RgbaImage,
    group: &[&crate::model::ModelFace],
    front: &RgbaImage,
    mirror: bool,
) {
    let face_rect = |name: &str| -> Option<FaceRect> {
        group
            .iter()
            .find(|face| face.face == name)
            .map(|face| face.rect)
    };
    let Some(north) = face_rect("north") else {
        return;
    };

    let source = if mirror {
        image::imageops::flip_horizontal(front)
    } else {
        front.clone()
    };

    // The front goes to the north face.
    blit(output, north, &source);

    // The other faces are derived from it.
    if let Some(south) = face_rect("south") {
        blit(output, south, &image::imageops::flip_horizontal(&source));
    }
    if let Some(up) = face_rect("up") {
        blit(output, up, &strip(&source, Strip::Top));
    }
    if let Some(down) = face_rect("down") {
        blit(output, down, &strip(&source, Strip::Bottom));
    }
    if let Some(west) = face_rect("west") {
        blit(output, west, &strip(&source, Strip::Left));
    }
    if let Some(east) = face_rect("east") {
        blit(output, east, &strip(&source, Strip::Right));
    }
}

/// Which edge of the front to take.
enum Strip {
    Top,
    Bottom,
    Left,
    Right,
}

/// The edge of the front, stretched to the size of the face.
fn strip(image: &RgbaImage, which: Strip) -> RgbaImage {
    if image.width() == 0 || image.height() == 0 {
        return RgbaImage::new(1, 1);
    }
    match which {
        Strip::Top => image::imageops::crop_imm(image, 0, 0, image.width(), 1).to_image(),
        Strip::Bottom => {
            image::imageops::crop_imm(image, 0, image.height() - 1, image.width(), 1).to_image()
        }
        Strip::Left => image::imageops::crop_imm(image, 0, 0, 1, image.height()).to_image(),
        Strip::Right => {
            image::imageops::crop_imm(image, image.width() - 1, 0, 1, image.height()).to_image()
        }
    }
}

/// Copy an image into a rectangle, scaling it to fit.
fn blit(output: &mut RgbaImage, rect: FaceRect, source: &RgbaImage) {
    if rect.width == 0 || rect.height == 0 || source.width() == 0 || source.height() == 0 {
        return;
    }
    for y in 0..rect.height {
        for x in 0..rect.width {
            let sx = (x as u64 * source.width() as u64 / rect.width as u64) as u32;
            let sy = (y as u64 * source.height() as u64 / rect.height as u64) as u32;
            let pixel = *source.get_pixel(sx.min(source.width() - 1), sy.min(source.height() - 1));
            let tx = rect.x + x;
            let ty = rect.y + y;
            if tx < output.width() && ty < output.height() {
                output.put_pixel(tx, ty, pixel);
            }
        }
    }
}

/// The pixel of a skin that is not covered by anything.
pub const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_model;
    use serde_json::json;

    fn humanoid_model() -> ModelFaces {
        parse_model(&json!({
            "texture_size": [64, 64],
            "parts": [
                {"name": "head", "cubes": [{"origin": [-4.0, -4.0, -4.0], "size": [8.0, 8.0, 8.0], "uv": [0.0, 0.0]}]},
                {"name": "body", "cubes": [{"origin": [-4.0, 0.0, -2.0], "size": [8.0, 12.0, 4.0], "uv": [16.0, 16.0]}]},
                {"name": "right_arm", "cubes": [{"origin": [-3.0, -2.0, -2.0], "size": [4.0, 12.0, 4.0], "uv": [40.0, 16.0]}]},
                {"name": "right_leg", "cubes": [{"origin": [-2.0, 0.0, -2.0], "size": [4.0, 12.0, 4.0], "uv": [0.0, 16.0]}]}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn the_classic_layout_is_right() {
        let parts = LegacyParts::classic();
        assert_eq!(parts.head.x, 8);
        assert_eq!(parts.body.x, 16);
        assert_eq!(parts.right_arm.x, 40);
        assert_eq!(parts.right_leg.x, 0);
    }

    #[test]
    fn rejects_a_skin_of_the_wrong_size() {
        let model = humanoid_model();
        let error = convert(&RgbaImage::new(32, 32), &model).unwrap_err();
        assert!(error.contains("64 pixels wide"));
    }

    #[test]
    fn converts_a_legacy_skin() {
        let model = humanoid_model();
        let mut legacy = RgbaImage::new(64, 32);
        // The head of the legacy skin is red.
        for y in 0..8 {
            for x in 8..16 {
                legacy.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let modern = convert(&legacy, &model).unwrap();
        assert_eq!(modern.dimensions(), (64, 64));
        // The head front ended up at the model's north face.
        let head = model
            .faces
            .iter()
            .find(|face| face.face == "north" && face.rect.x == 8 && face.rect.y == 8)
            .expect("the head front");
        let pixel = modern.get_pixel(head.rect.x, head.rect.y).0;
        assert_eq!(pixel[0], 255, "the head should be red");
        assert_eq!(pixel[3], 255);
    }
}
