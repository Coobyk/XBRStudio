use std::collections::HashMap;

use image::RgbaImage;
use serde::Deserialize;
use serde_json::Value;

pub const MODEL_UV_MAX: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl FaceRect {
    pub fn right(&self) -> u32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> u32 {
        self.y + self.height
    }
}

#[derive(Debug, Clone)]
pub struct ModelFace {
    #[allow(dead_code)]
    pub texture: String,
    pub rect: FaceRect,
    pub face: String,
    /// Cube/element index within the model; neighbors are only looked up
    /// inside this group.
    pub group: u32,
}

#[derive(Debug, Clone)]
pub struct ModelFaces {
    #[allow(dead_code)]
    pub textures: HashMap<String, String>,
    pub faces: Vec<ModelFace>,
    /// UV space size: block models use 16×16; entity models use `texture_size`.
    pub uv_size: (f32, f32),
}

#[derive(Deserialize)]
struct RawModel {
    #[serde(default)]
    elements: Vec<RawElement>,
}

#[derive(Deserialize)]
struct RawElement {
    #[serde(default)]
    faces: HashMap<String, RawFace>,
}

#[derive(Deserialize)]
struct RawFace {
    #[serde(default)]
    uv: Option<[f32; 4]>,
    #[serde(default)]
    texture: Option<String>,
}

#[derive(Deserialize)]
struct RawEntityModel {
    #[serde(default)]
    texture_size: Option<[f32; 2]>,
    #[serde(default)]
    parts: Vec<RawEntityPart>,
}

#[derive(Deserialize)]
struct RawEntityPart {
    #[serde(default)]
    cubes: Vec<RawEntityCube>,
    #[serde(default)]
    children: Vec<RawEntityPart>,
}

#[derive(Deserialize)]
struct RawEntityCube {
    #[serde(default)]
    size: [f32; 3],
    #[serde(default)]
    uv: [f32; 2],
}

pub fn load_model(path: &std::path::Path) -> Result<ModelFaces, String> {
    let data = std::fs::read_to_string(path).map_err(|e| format!("cannot read model: {e}"))?;
    let root: Value =
        serde_json::from_str(&data).map_err(|e| format!("cannot parse model: {e}"))?;
    parse_model(&root)
}

pub fn parse_model(root: &Value) -> Result<ModelFaces, String> {
    if root.get("parts").is_some() {
        return parse_entity_model(root);
    }
    parse_block_model(root)
}

fn parse_block_model(root: &Value) -> Result<ModelFaces, String> {
    let textures = extract_textures(root);

    let model: RawModel = serde_json::from_value(root.clone())
        .map_err(|e| format!("unsupported block model structure: {e}"))?;

    let mut faces = Vec::new();
    for (element_index, element) in model.elements.into_iter().enumerate() {
        for (face_name, face) in element.faces {
            if let Some(uv) = face.uv {
                let texture = face
                    .texture
                    .map(|value| value.trim_start_matches('#').to_string())
                    .or_else(|| first_texture_reference(&textures))
                    .unwrap_or_default();

                faces.push(ModelFace {
                    texture,
                    rect: FaceRect {
                        x: uv[0].round() as u32,
                        y: uv[1].round() as u32,
                        width: (uv[2] - uv[0]).round().max(0.0) as u32,
                        height: (uv[3] - uv[1]).round().max(0.0) as u32,
                    },
                    face: face_name,
                    group: element_index as u32,
                });
            }
        }
    }

    Ok(ModelFaces {
        textures,
        faces,
        uv_size: (MODEL_UV_MAX, MODEL_UV_MAX),
    })
}

fn parse_entity_model(root: &Value) -> Result<ModelFaces, String> {
    let model: RawEntityModel = serde_json::from_value(root.clone())
        .map_err(|e| format!("unsupported entity model structure: {e}"))?;

    let uv_size = model
        .texture_size
        .map(|size| (size[0].max(1.0), size[1].max(1.0)))
        .unwrap_or((MODEL_UV_MAX, MODEL_UV_MAX));

    let mut faces = Vec::new();
    let textures = HashMap::new();
    let mut group_counter = 0u32;
    for part in &model.parts {
        collect_entity_faces(part, &textures, &mut faces, &mut group_counter);
    }

    Ok(ModelFaces {
        textures,
        faces,
        uv_size,
    })
}

fn collect_entity_faces(
    part: &RawEntityPart,
    textures: &HashMap<String, String>,
    out: &mut Vec<ModelFace>,
    group_counter: &mut u32,
) {
    for cube in &part.cubes {
        let [dx, dy, dz] = cube.size;
        let (u, v) = (cube.uv[0], cube.uv[1]);
        let u0 = u;
        let u1 = u + dz;
        let u2 = u + dz + dx;
        let u3 = u + dz + dx + dz;
        let v0 = v;
        let v1 = v + dz;

        // Flat cubes (any dimension ≤ 0) still have non-degenerate faces:
        // dy=0 keeps up/down (w=dx, h=dz), dx=0 keeps east/west, dz=0 keeps
        // north/south. Emit only the faces with positive area instead of
        // skipping the whole cube (frog tongue/feet, bat wings, fish fins, …).
        let rects = [
            ("down", u1, v0, dx, dz),
            ("up", u2, v0, dx, dz),
            ("east", u2, v1, dz, dy),
            ("north", u1, v1, dx, dy),
            ("west", u0, v1, dz, dy),
            ("south", u3, v1, dx, dy),
        ];

        let mut emitted = false;
        let group = *group_counter;
        for (face_name, x, y, w, h) in rects {
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            if !emitted {
                *group_counter += 1;
                emitted = true;
            }
            out.push(ModelFace {
                texture: first_texture_reference(textures).unwrap_or_default(),
                rect: FaceRect {
                    x: x.round().max(0.0) as u32,
                    y: y.round().max(0.0) as u32,
                    width: w.round().max(0.0) as u32,
                    height: h.round().max(0.0) as u32,
                },
                face: face_name.into(),
                group,
            });
        }
    }

    for child in &part.children {
        collect_entity_faces(child, textures, out, group_counter);
    }
}

fn extract_textures(root: &Value) -> HashMap<String, String> {
    root.get("textures")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn first_texture_reference(textures: &HashMap<String, String>) -> Option<String> {
    textures.keys().next().map(|value| value.to_string())
}

pub fn scale_model_faces_to_image(model: &mut ModelFaces, image: &RgbaImage) {
    let (uv_w, uv_h) = model.uv_size;
    let x_scale = image.width() as f32 / uv_w;
    let y_scale = image.height() as f32 / uv_h;

    model.faces.retain_mut(|model_face| {
        let rect = &mut model_face.rect;
        let x = (rect.x as f32 * x_scale).floor() as i64;
        let y = (rect.y as f32 * y_scale).floor() as i64;
        let right = (rect.right() as f32 * x_scale).ceil() as i64;
        let bottom = (rect.bottom() as f32 * y_scale).ceil() as i64;

        let width = i64::from(image.width());
        let height = i64::from(image.height());
        if x >= width || y >= height {
            rect.width = 0;
            rect.height = 0;
            return false;
        }

        let right = right.clamp(x, width);
        let bottom = bottom.clamp(y, height);
        let x = x.clamp(0, right);
        let y = y.clamp(0, bottom);

        rect.x = x as u32;
        rect.y = y as u32;
        rect.width = (right - x) as u32;
        rect.height = (bottom - y) as u32;

        rect.width > 0 && rect.height > 0
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_block_faces_and_textures() {
        let model = json!({
            "textures": {"all": "minecraft:block/stone"},
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "north": {"uv": [0, 0, 16, 8]},
                    "east": {"uv": [16, 0, 24, 8]}
                }
            }]
        });

        let parsed = parse_model(&model).unwrap();
        assert_eq!(parsed.faces.len(), 2);
        assert_eq!(parsed.uv_size, (16.0, 16.0));
        assert_eq!(parsed.textures.get("all").unwrap(), "minecraft:block/stone");
        let north = parsed.faces.iter().find(|face| face.face == "north").unwrap();
        let east = parsed.faces.iter().find(|face| face.face == "east").unwrap();
        assert_eq!(north.rect, FaceRect { x: 0, y: 0, width: 16, height: 8 });
        assert_eq!(east.rect, FaceRect { x: 16, y: 0, width: 8, height: 8 });
    }

    #[test]
    fn parses_entity_box_uv() {
        // Cow head: size 8×8×6, uv (0,0) → six faces per ModelPart box layout.
        let model = json!({
            "texture_size": [64, 64],
            "parts": [{
                "name": "head",
                "translation": [0, 4, -8],
                "rotation": [0, 0, 0],
                "cubes": [{
                    "origin": [-4, -4, -6],
                    "size": [8, 8, 6],
                    "uv": [0, 0],
                    "mirror": false
                }]
            }]
        });

        let parsed = parse_model(&model).unwrap();
        assert_eq!(parsed.uv_size, (64.0, 64.0));
        assert_eq!(parsed.faces.len(), 6);

        let find = |name: &str| {
            parsed
                .faces
                .iter()
                .find(|face| face.face == name)
                .unwrap()
                .rect
        };
        // dx=8, dy=8, dz=6
        assert_eq!(find("down"), FaceRect { x: 6, y: 0, width: 8, height: 6 });
        assert_eq!(find("up"), FaceRect { x: 14, y: 0, width: 8, height: 6 });
        assert_eq!(find("east"), FaceRect { x: 14, y: 6, width: 6, height: 8 });
        assert_eq!(find("north"), FaceRect { x: 6, y: 6, width: 8, height: 8 });
        assert_eq!(find("west"), FaceRect { x: 0, y: 6, width: 6, height: 8 });
        assert_eq!(find("south"), FaceRect { x: 20, y: 6, width: 8, height: 8 });
    }

    #[test]
    fn entity_scale_is_identity_on_matching_texture() {
        let model = json!({
            "texture_size": [64, 64],
            "parts": [{
                "name": "body",
                "translation": [0, 5, 2],
                "rotation": ["(float) (Math.PI / 2)", 0, 0],
                "cubes": [{
                    "origin": [-6, -10, -7],
                    "size": [12, 18, 10],
                    "uv": [18, 4],
                    "mirror": false
                }]
            }]
        });
        let mut parsed = parse_model(&model).unwrap();
        let before = parsed.faces.clone();
        let image = RgbaImage::new(64, 64);
        scale_model_faces_to_image(&mut parsed, &image);
        assert_eq!(parsed.faces.len(), before.len());
        for (a, b) in before.iter().zip(parsed.faces.iter()) {
            assert_eq!(a.rect, b.rect);
        }
    }

    #[test]
    fn clips_faces_to_image() {
        let mut model = ModelFaces {
            textures: HashMap::new(),
            faces: vec![
                ModelFace {
                    texture: String::new(),
                    rect: FaceRect {
                        x: 0,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    face: "north".into(),
                    group: 0,
                },
                ModelFace {
                    texture: String::new(),
                    rect: FaceRect {
                        x: 16,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    face: "east".into(),
                    group: 1,
                },
            ],
            uv_size: (16.0, 16.0),
        };
        let image = RgbaImage::new(16, 16);
        scale_model_faces_to_image(&mut model, &image);
        assert_eq!(model.faces.len(), 1);
        assert_eq!(model.faces[0].rect.width, 16);
    }

    #[test]
    fn drops_faces_starting_beyond_image() {
        let mut model = ModelFaces {
            textures: HashMap::new(),
            faces: vec![ModelFace {
                texture: String::new(),
                rect: FaceRect {
                    x: 80,
                    y: 0,
                    width: 16,
                    height: 16,
                },
                face: "north".into(),
                group: 0,
            }],
            uv_size: (64.0, 64.0),
        };
        let image = RgbaImage::new(32, 32);
        scale_model_faces_to_image(&mut model, &image);
        assert!(model.faces.is_empty());
    }

    #[test]
    fn assigns_group_per_box() {
        let entity = json!({
            "texture_size": [64, 64],
            "parts": [{
                "name": "head",
                "cubes": [
                    {"origin": [0, 0, 0], "size": [8, 8, 6], "uv": [0, 0]},
                    {"origin": [0, 0, 0], "size": [2, 3, 1], "uv": [22, 0]}
                ]
            }]
        });
        let parsed = parse_model(&entity).unwrap();
        let groups: std::collections::HashSet<u32> =
            parsed.faces.iter().map(|face| face.group).collect();
        assert_eq!(groups, [0, 1].into_iter().collect());
        assert!(parsed
            .faces
            .iter()
            .filter(|face| face.group == 0)
            .all(|face| matches!(face.face.as_str(), "down" | "up" | "east" | "north" | "west" | "south")));

        let block = json!({
            "elements": [
                {"from": [0, 0, 0], "to": [16, 16, 16],
                 "faces": {"north": {"uv": [0, 0, 16, 16]}}},
                {"from": [0, 0, 0], "to": [8, 8, 8],
                 "faces": {"east": {"uv": [0, 0, 8, 8]}}}
            ]
        });
        let parsed = parse_model(&block).unwrap();
        let groups: std::collections::HashSet<u32> =
            parsed.faces.iter().map(|face| face.group).collect();
        assert_eq!(groups, [0, 1].into_iter().collect());
    }

    #[test]
    fn flat_cube_emits_non_degenerate_faces() {
        // Frog-style dy=0 plate: only up/down have area (w=dx, h=dz).
        let entity = json!({
            "texture_size": [48, 48],
            "parts": [{
                "name": "tongue",
                "cubes": [{
                    "origin": [-2.0, 0.0, -7.1],
                    "size": [4.0, 0.0, 7.0],
                    "uv": [17.0, 13.0]
                }]
            }]
        });
        let parsed = parse_model(&entity).unwrap();
        assert_eq!(parsed.faces.len(), 2, "dy=0 cube should emit up+down only");
        let down = parsed
            .faces
            .iter()
            .find(|face| face.face == "down")
            .expect("down face");
        let up = parsed
            .faces
            .iter()
            .find(|face| face.face == "up")
            .expect("up face");
        // dx=4, dz=7, uv(17,13): down at (u+dz, v)=(24,13) size 4×7;
        // up at (u+dz+dx, v)=(28,13) size 4×7.
        assert_eq!(
            down.rect,
            FaceRect { x: 24, y: 13, width: 4, height: 7 }
        );
        assert_eq!(
            up.rect,
            FaceRect { x: 28, y: 13, width: 4, height: 7 }
        );
        assert_eq!(down.group, up.group);

        // dx=0 slab: only east/west have area (w=dz, h=dy).
        let entity = json!({
            "texture_size": [64, 64],
            "parts": [{
                "name": "wing",
                "cubes": [{
                    "origin": [0.0, 0.0, 0.0],
                    "size": [0.0, 5.0, 8.0],
                    "uv": [16.0, 0.0]
                }]
            }]
        });
        let parsed = parse_model(&entity).unwrap();
        let names: Vec<&str> = parsed.faces.iter().map(|face| face.face.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"east") && names.contains(&"west"));
        let east = parsed
            .faces
            .iter()
            .find(|face| face.face == "east")
            .unwrap();
        // uv(16,0), dz=8, dy=5: east at (u+dz+dx, v+dz)=(24, 8) size 8×5.
        assert_eq!(
            east.rect,
            FaceRect { x: 24, y: 8, width: 8, height: 5 }
        );
    }
}
