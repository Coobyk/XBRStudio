#!/usr/bin/env python3
"""Collect the item models and the atlases in the archive reader."""
import sys

p = "src/jar.rs"
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
    """    let mut block_model_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {""",
    """    let mut block_model_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut item_model_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut atlas_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {""",
)

swap(
    """        } else if name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json") {
            let short = name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if !short.is_empty() {
                let bytes = read_entry(&mut archive, index)?;
                block_model_entries.push((short, bytes));
            }
        }
    }""",
    """        } else if name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json") {
            let short = name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if !short.is_empty() {
                let bytes = read_entry(&mut archive, index)?;
                block_model_entries.push((short, bytes));
            }
        } else if name.starts_with(ITEM_MODEL_ROOT) && name.ends_with(".json") {
            let short = name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if !short.is_empty() {
                let bytes = read_entry(&mut archive, index)?;
                item_model_entries.push((short, bytes));
            }
        } else if name.starts_with(ATLAS_ROOT) && name.ends_with(".json") {
            let bytes = read_entry(&mut archive, index)?;
            atlas_entries.push((name, bytes));
        }
    }""",
)

open(p, "w").write(s)
print("archive reader collects the assets")
