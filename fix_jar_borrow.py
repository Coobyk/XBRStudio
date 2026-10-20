#!/usr/bin/env python3
"""Fix the borrow in load_block_model."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """    let mut raw: HashMap<String, Value> = HashMap::new();
    let mut index_by_name: HashMap<String, usize> = HashMap::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("cannot list jar entry: {error}"))?;
        let entry_name = entry.name().to_string();
        index_by_name.insert(entry_name.clone(), index);
        if entry_name.starts_with(BLOCK_MODEL_ROOT) && entry_name.ends_with(".json") {
            let short = entry_name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if short.is_empty() {
                continue;
            }
            let bytes = read_entry(&mut archive, index)?;
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                raw.insert(short, value);
            }
        }
    }"""

new = """    let mut raw: HashMap<String, Value> = HashMap::new();
    let mut entries: Vec<(String, usize)> = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("cannot list jar entry: {error}"))?;
        let entry_name = entry.name().to_string();
        if entry_name.starts_with(BLOCK_MODEL_ROOT) && entry_name.ends_with(".json") {
            let short = entry_name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if !short.is_empty() {
                entries.push((short, index));
            }
        }
    }

    for (short, index) in entries {
        let bytes = read_entry(&mut archive, index)?;
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            raw.insert(short, value);
        }
    }"""

assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("borrow fixed")
