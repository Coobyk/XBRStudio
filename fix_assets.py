#!/usr/bin/env python3
"""Fix the atlas, blockstate and item model parsing."""
import sys

p = "src/assets.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:400])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:400]))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """            // `minecraft:block/stone` -> `block/stone`
            let path = source
                .rsplit(':')
                .next()
                .unwrap_or(source)
                .trim_start_matches('/');
            let path = path.strip_prefix("textures/").unwrap_or(path);
            if path.ends_with(".png") {
                sprites.push(path.to_string());
            }""",
    """            // `minecraft:block/stone` -> `block/stone.png`
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
            sprites.push(format!("{path}.png"));""",
)

swap(
    """/// The models of every block, from the block states.""",
    """/// The textures a model uses, by their file stem.
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

/// The models of every block, from the block states.""",
)

swap(
    """        for stem in crate::model::block_texture_stems(&parsed.textures) {""",
    """        for stem in texture_stems(&parsed.textures) {""",
)

swap(
    """fn collect_models(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(model) = map.get("model") {
                collect_model_name(model, out);
            }
            for (key, child) in map {
                if key == "model" {
                    continue;
                }
                if key == "variants" {
                    collect_models(child, out);
                } else if key == "multipart" {
                    if let Some(parts) = child.as_array() {
                        for part in parts {
                            collect_models(part, out);
                        }
                    }
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
}""",
    """fn collect_models(value: &Value, out: &mut Vec<String>) {
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
}""",
)

open(p, "w").write(s)
print("assets.rs fixed")
