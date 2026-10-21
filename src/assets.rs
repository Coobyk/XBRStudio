//! The assets of the game: the atlases, the item models and the block
//! states.
//!
//! The client jar describes more than the textures: the atlases say which
//! sprites exist, the item models say how an item looks and the block states
//! say which model a block uses. Reading them gives the pack a better idea of
//! what belongs together.

use std::collections::HashMap;

use serde_json::Value;

use crate::model::{ModelFaces, parse_model, resolve_block_models};

/// The sprites of the atlases of a jar: the texture paths they hold.
pub fn parse_atlases(entries: &[(String, Vec<u8>)]) -> Vec<String> {
    let mut sprites = Vec::new();
    for (name, bytes) in entries {
        if !name.starts_with("assets/minecraft/atlases/") || !name.ends_with(".json") {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        let Some(sources) = value.get("sources").and_then(Value::as_array) else {
            continue;
        };
        for source in sources {
            let Some(source) = source.get("source") else {
                continue;
            };
            let Some(source) = source.as_str() else {
                continue;
            };
            if source.starts_with("#") {
                continue;
            }
            // `minecraft:block/stone` -> `block/stone.png`
            let path = source
                .rsplit(':')
                .next()
                .unwrap_or(source)
                .trim_start_matches('/');
            let path = path.strip_prefix("textures/").unwrap_or(path);
            if path.is_empty() {
                continue;
            }
            let path = path.strip_suffix(".png").unwrap_or(path);
            sprites.push(format!("{path}.png"));
        }
    }
    sprites.sort();
    sprites.dedup();
    sprites
}

/// The item models of a jar, by the texture they use.
pub fn load_item_models(entries: &[(String, Vec<u8>)]) -> HashMap<String, ModelFaces> {
    let mut raw: HashMap<String, Value> = HashMap::new();
    for (name, bytes) in entries {
        if !name.starts_with("assets/minecraft/models/item/") || !name.ends_with(".json") {
            continue;
        }
        let short = name
            .trim_end_matches(".json")
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        if short.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
            raw.insert(short, value);
        }
    }

    let resolved = resolve_block_models(&raw);
    let mut by_stem: HashMap<String, Vec<(String, ModelFaces)>> = HashMap::new();
    let mut entries: Vec<(&String, &Value)> = resolved.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in entries {
        let Ok(parsed) = parse_model(value) else {
            continue;
        };
        if parsed.faces.is_empty() {
            continue;
        }
        for stem in texture_stems(&parsed.textures) {
            by_stem
                .entry(stem)
                .or_default()
                .push((key.clone(), parsed.clone()));
        }
    }

    by_stem
        .into_iter()
        .map(|(stem, models)| {
            let mut models = models;
            models.sort_by(|a, b| a.0.cmp(&b.0));
            (stem, models[0].1.clone())
        })
        .collect()
}

/// The textures a model uses, by their file stem.
///
/// Block and item textures are both indexed by their stem, so a model that
/// uses `minecraft:item/diamond_sword` is found under `diamond_sword`.
fn texture_stems(textures: &HashMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut keys: Vec<&String> = textures.keys().collect();
    keys.sort();
    for key in keys {
        let value = &textures[key];
        let path = value
            .rsplit(':')
            .next()
            .unwrap_or(value)
            .trim_start_matches('/');
        let path = path.strip_prefix("textures/").unwrap_or(path);
        let Some(stem) = path.rsplit('/').next() else {
            continue;
        };
        let stem = stem.strip_suffix(".png").unwrap_or(stem);
        if stem.is_empty() {
            continue;
        }
        if !out.contains(&stem.to_string()) {
            out.push(stem.to_string());
        }
    }
    out
}

/// The models of every block, from the block states.
///
/// The value is the list of the model names of the block, the states that
/// use another model included.
pub fn parse_blockstates(entries: &[(String, Vec<u8>)]) -> HashMap<String, Vec<String>> {
    let mut by_block: HashMap<String, Vec<String>> = HashMap::new();
    for (name, bytes) in entries {
        if !name.starts_with("assets/minecraft/blockstates/") || !name.ends_with(".json") {
            continue;
        }
        let block = name
            .trim_end_matches(".json")
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        if block.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        let mut models = Vec::new();
        collect_models(&value, &mut models);
        models.sort();
        models.dedup();
        by_block.insert(block, models);
    }
    by_block
}

/// The model names of a block state.
fn collect_models(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(model) = map.get("model") {
                collect_model_name(model, out);
            }
            if let Some(Value::Object(variants)) = map.get("variants") {
                for variant in variants.values() {
                    collect_models(variant, out);
                }
            }
            if let Some(Value::Array(parts)) = map.get("multipart") {
                for part in parts {
                    collect_models(part, out);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_models(item, out);
            }
        }
        _ => {}
    }
}

/// The name of a model reference.
fn collect_model_name(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(name) => {
            let name = name
                .rsplit(':')
                .next()
                .unwrap_or(name)
                .trim_start_matches('/');
            out.push(name.to_string());
        }
        Value::Object(map) => {
            if let Some(name) = map.get("model") {
                collect_model_name(name, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_sprites_of_an_atlas() {
        let entries = vec![(
            "assets/minecraft/atlases/blocks.json".to_string(),
            serde_json::to_vec(&json!({
                "sources": [
                    {"source": "minecraft:block/stone"},
                    {"source": "#minecraft:placeholder"},
                    {"source": "minecraft:item/diamond_sword"}
                ]
            }))
            .unwrap(),
        )];
        let sprites = parse_atlases(&entries);
        assert_eq!(sprites, vec!["block/stone.png", "item/diamond_sword.png"]);
    }

    #[test]
    fn reads_the_models_of_a_blockstate() {
        let entries = vec![(
            "assets/minecraft/blockstates/oak_log.json".to_string(),
            serde_json::to_vec(&json!({
                "variants": {
                    "axis=y": {"model": "minecraft:block/oak_log"},
                    "axis=x": {"model": "minecraft:block/oak_log", "x": 90}
                }
            }))
            .unwrap(),
        )];
        let states = parse_blockstates(&entries);
        assert_eq!(states.get("oak_log").unwrap(), &vec!["block/oak_log"]);
    }

    #[test]
    fn reads_an_item_model() {
        // The item models are mostly flat, only a few have geometry.
        let entries = vec![(
            "assets/minecraft/models/item/spyglass_in_hand.json".to_string(),
            serde_json::to_vec(&json!({
                "textures": {"spyglass": "minecraft:item/spyglass_model"},
                "elements": [{
                    "from": [7, 8.5, 7],
                    "to": [9, 13.5, 9],
                    "faces": {"north": {"uv": [0, 2, 2, 7]}}
                }]
            }))
            .unwrap(),
        )];
        let models = load_item_models(&entries);
        assert!(models.contains_key("spyglass_model"), "{models:?}");
    }
}
