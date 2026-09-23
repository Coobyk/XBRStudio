use std::collections::{HashMap, HashSet};

use image::RgbaImage;
use serde::Deserialize;
use serde_json::Value;

pub const MODEL_UV_MAX: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
            let texture = face
                .texture
                .map(|value| value.trim_start_matches('#').to_string())
                .or_else(|| first_texture_reference(&textures))
                .unwrap_or_default();
            // Faces that only set `texture` (cube parents) default to the full
            // 16×16 UV, matching vanilla when `uv` is omitted.
            let uv = match face.uv {
                Some(uv) => uv,
                None => {
                    if texture.is_empty() {
                        continue;
                    }
                    [0.0, 0.0, MODEL_UV_MAX, MODEL_UV_MAX]
                }
            };

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

    Ok(ModelFaces {
        textures,
        faces,
        uv_size: (MODEL_UV_MAX, MODEL_UV_MAX),
    })
}

/// Resolve `parent` chains in raw block-model JSON. Child `textures` override
/// the parent; child `elements` win when present, otherwise the parent's
/// elements are inherited. Keys are short names (`cube_all`, `lantern`).
pub fn resolve_block_models(raw: &HashMap<String, Value>) -> HashMap<String, Value> {
    let mut resolved = HashMap::with_capacity(raw.len());
    for key in raw.keys() {
        resolve_block_model_one(key, raw, &mut resolved, &mut HashSet::new());
    }
    resolved
}

fn resolve_block_model_one(
    key: &str,
    raw: &HashMap<String, Value>,
    resolved: &mut HashMap<String, Value>,
    visiting: &mut HashSet<String>,
) -> Option<Value> {
    if let Some(value) = resolved.get(key) {
        return Some(value.clone());
    }
    if !visiting.insert(key.to_string()) {
        return None;
    }
    let mut value = raw.get(key)?.clone();
    let parent = value
        .get("parent")
        .and_then(Value::as_str)
        .map(parent_model_key);
    if let Some(parent_key) = parent {
        let parent_value = resolve_block_model_one(&parent_key, raw, resolved, visiting)?;
        value = merge_block_parent(&parent_value, &value);
    }
    visiting.remove(key);
    let out = value.clone();
    resolved.insert(key.to_string(), value);
    Some(out)
}

fn parent_model_key(parent: &str) -> String {
    parent
        .rsplit('/')
        .next()
        .unwrap_or(parent)
        .trim_start_matches("minecraft:")
        .to_string()
}

fn merge_block_parent(parent: &Value, child: &Value) -> Value {
    let mut merged = parent.clone();
    if let (Some(parent_obj), Some(child_obj)) =
        (merged.as_object_mut(), child.as_object().cloned())
    {
        for (key, value) in child_obj {
            if key == "textures" {
                let mut textures = parent_obj
                    .get("textures")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                if let Some(child_textures) = value.as_object() {
                    for (name, tex) in child_textures {
                        textures.insert(name.clone(), tex.clone());
                    }
                }
                parent_obj.insert(key, Value::Object(textures));
            } else if key == "elements" {
                let has_child_elements = value
                    .as_array()
                    .is_some_and(|elements| !elements.is_empty());
                if has_child_elements || !parent_obj.contains_key("elements") {
                    parent_obj.insert(key, value);
                }
            } else {
                parent_obj.insert(key, value);
            }
        }
    }
    if let Some(obj) = merged.as_object_mut() {
        obj.remove("parent");
    }
    merged
}

/// Atlas-like layout: at least one element (group) uses two or more distinct
/// UV rects. Full-UV cubes (`cube_all`) keep every face on the same rect and
/// must fall back to wrap — 3D neighbors are not atlas neighbors there.
pub fn is_atlas_layout(faces: &[ModelFace]) -> bool {
    if faces.len() < 2 {
        return false;
    }
    let mut by_group: HashMap<u32, Vec<FaceRect>> = HashMap::new();
    for face in faces {
        by_group.entry(face.group).or_default().push(face.rect);
    }
    by_group.values().any(|rects| {
        if rects.len() < 2 {
            return false;
        }
        let unique: HashSet<FaceRect> = rects.iter().copied().collect();
        unique.len() >= 2
    })
}

/// Merge models that share a texture; group ids are shifted so neighbor
/// lookups never cross elements from different source models.
pub fn merge_model_faces(models: &[ModelFaces]) -> Option<ModelFaces> {
    if models.is_empty() {
        return None;
    }
    let mut faces = Vec::new();
    let mut textures = HashMap::new();
    let mut group_offset = 0u32;
    for model in models {
        let max_group = model.faces.iter().map(|face| face.group).max();
        for face in &model.faces {
            let mut face = face.clone();
            face.group += group_offset;
            faces.push(face);
        }
        group_offset += max_group.map(|group| group + 1).unwrap_or(0);
        textures.extend(model.textures.clone());
    }
    if faces.is_empty() {
        return None;
    }
    Some(ModelFaces {
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
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn first_texture_reference(textures: &HashMap<String, String>) -> Option<String> {
    textures.keys().next().map(|value| value.to_string())
}

/// Concrete `block/<name>` stems referenced by a model's texture map
/// (follows `#alias` chains).
pub fn block_texture_stems(textures: &HashMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for value in textures.values() {
        collect_block_stems(textures, value, &mut out, &mut seen, 0);
    }
    out
}

fn collect_block_stems(
    textures: &HashMap<String, String>,
    value: &str,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
    depth: usize,
) {
    if depth > 8 || value.is_empty() {
        return;
    }
    if let Some(alias) = value.strip_prefix('#') {
        if let Some(next) = textures.get(alias) {
            collect_block_stems(textures, next, out, seen, depth + 1);
        }
        return;
    }
    if let Some(stem) = block_texture_stem(value)
        && seen.insert(stem.clone())
    {
        out.push(stem);
    }
}

/// `minecraft:block/lantern` / `block/lantern` → `lantern`.
pub fn block_texture_stem(value: &str) -> Option<String> {
    let path = value.rsplit(':').next()?;
    let path = path.strip_prefix("block/")?;
    if path.is_empty() || path.contains('/') {
        return None;
    }
    Some(path.to_string())
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
        let north = parsed
            .faces
            .iter()
            .find(|face| face.face == "north")
            .unwrap();
        let east = parsed
            .faces
            .iter()
            .find(|face| face.face == "east")
            .unwrap();
        assert_eq!(
            north.rect,
            FaceRect {
                x: 0,
                y: 0,
                width: 16,
                height: 8
            }
        );
        assert_eq!(
            east.rect,
            FaceRect {
                x: 16,
                y: 0,
                width: 8,
                height: 8
            }
        );
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
        assert_eq!(
            find("down"),
            FaceRect {
                x: 6,
                y: 0,
                width: 8,
                height: 6
            }
        );
        assert_eq!(
            find("up"),
            FaceRect {
                x: 14,
                y: 0,
                width: 8,
                height: 6
            }
        );
        assert_eq!(
            find("east"),
            FaceRect {
                x: 14,
                y: 6,
                width: 6,
                height: 8
            }
        );
        assert_eq!(
            find("north"),
            FaceRect {
                x: 6,
                y: 6,
                width: 8,
                height: 8
            }
        );
        assert_eq!(
            find("west"),
            FaceRect {
                x: 0,
                y: 6,
                width: 6,
                height: 8
            }
        );
        assert_eq!(
            find("south"),
            FaceRect {
                x: 20,
                y: 6,
                width: 8,
                height: 8
            }
        );
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
        assert!(
            parsed
                .faces
                .iter()
                .filter(|face| face.group == 0)
                .all(|face| matches!(
                    face.face.as_str(),
                    "down" | "up" | "east" | "north" | "west" | "south"
                ))
        );

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
            FaceRect {
                x: 24,
                y: 13,
                width: 4,
                height: 7
            }
        );
        assert_eq!(
            up.rect,
            FaceRect {
                x: 28,
                y: 13,
                width: 4,
                height: 7
            }
        );
        assert_eq!(down.group, up.group);
    }

    #[test]
    fn resolve_block_models_merges_parent_textures_and_elements() {
        let mut raw = HashMap::new();
        raw.insert("block".to_string(), json!({"gui_light": "side"}));
        raw.insert(
            "cube".to_string(),
            json!({
                "parent": "block/block",
                "elements": [{
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "north": {"texture": "#all"}
                    }
                }]
            }),
        );
        raw.insert(
            "stone".to_string(),
            json!({
                "parent": "minecraft:block/cube_all",
                "textures": {"all": "minecraft:block/stone"}
            }),
        );
        raw.insert(
            "cube_all".to_string(),
            json!({
                "parent": "block/cube",
                "textures": {
                    "particle": "#all",
                    "north": "#all",
                    "south": "#all"
                }
            }),
        );

        let resolved = resolve_block_models(&raw);
        let stone = resolved.get("stone").unwrap();
        assert!(stone.get("parent").is_none());
        assert_eq!(stone["textures"]["all"], json!("minecraft:block/stone"));
        assert_eq!(stone["textures"]["north"], json!("#all"));
        assert!(stone["elements"].is_array());
        assert_eq!(
            stone["elements"][0]["faces"]["north"]["texture"],
            json!("#all")
        );

        let parsed = parse_model(stone).unwrap();
        assert_eq!(parsed.faces.len(), 1);
        assert!(
            !is_atlas_layout(&parsed.faces),
            "single-face cube is not atlas"
        );
    }

    #[test]
    fn is_atlas_layout_requires_distinct_rects_in_one_group() {
        let full = ModelFace {
            texture: "all".into(),
            rect: FaceRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            face: "north".into(),
            group: 0,
        };
        let mut same = full.clone();
        same.face = "east".into();
        assert!(!is_atlas_layout(&[full.clone(), same]));

        let mut atlas = full.clone();
        atlas.rect = FaceRect {
            x: 8,
            y: 0,
            width: 8,
            height: 16,
        };
        assert!(is_atlas_layout(&[full, atlas]));
    }

    #[test]
    fn block_texture_stem_parses_namespaced_paths() {
        assert_eq!(
            block_texture_stem("minecraft:block/lantern").as_deref(),
            Some("lantern")
        );
        assert_eq!(block_texture_stem("block/dirt").as_deref(), Some("dirt"));
        assert_eq!(block_texture_stem("minecraft:item/stick"), None);
        assert_eq!(block_texture_stem("block/sub/dir"), None);
    }

    #[test]
    fn block_texture_stems_follows_aliases() {
        let mut textures = HashMap::new();
        textures.insert("particle".into(), "#lantern".to_string());
        textures.insert("lantern".into(), "minecraft:block/lantern".to_string());
        let stems = block_texture_stems(&textures);
        assert_eq!(stems, vec!["lantern".to_string()]);
    }

    #[test]
    fn texture_only_faces_default_to_full_uv() {
        let model = json!({
            "textures": {"all": "minecraft:block/dirt"},
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "north": {"texture": "#all"},
                    "east": {"texture": "#all"}
                }
            }]
        });
        let parsed = parse_model(&model).unwrap();
        assert_eq!(parsed.faces.len(), 2);
        assert!(
            parsed
                .faces
                .iter()
                .all(|face| face.rect.width == 16 && face.rect.height == 16)
        );
        assert!(!is_atlas_layout(&parsed.faces));
    }
}
