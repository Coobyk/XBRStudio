#!/usr/bin/env python3
"""Collect the item models and the atlases of the source."""
import re
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


# The archive reader.
swap(
    """    let mut index_by_name: HashMap<String, usize> = HashMap::with_capacity(archive.len());
    let mut textures: Vec<(usize, String)> = Vec::new();
    let mut block_model_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {""",
    """    let mut index_by_name: HashMap<String, usize> = HashMap::with_capacity(archive.len());
    let mut textures: Vec<(usize, String)> = Vec::new();
    let mut block_model_entries: Vec<(String, Vec<u8>)> = Vec::new();
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

swap(
    """    let pack_format = read_pack_format(&mut archive);
    Ok(Source {
        jobs,
        block_model_entries,
        pack_format,
    })
}""",
    """    let pack_format = read_pack_format(&mut archive);
    Ok(Source {
        jobs,
        block_model_entries,
        item_model_entries,
        atlas_entries,
        pack_format,
    })
}""",
)

# The directory reader.
swap(
    """    let mut jobs = Vec::new();
    let mut block_model_entries = Vec::new();
    let mut stack = vec![dir.to_path_buf()];""",
    """    let mut jobs = Vec::new();
    let mut block_model_entries = Vec::new();
    let mut item_model_entries = Vec::new();
    let mut atlas_entries = Vec::new();
    let mut stack = vec![dir.to_path_buf()];""",
)

swap(
    """            } else if name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json") {
                let short = name
                    .trim_end_matches(".json")
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                if short.is_empty() {
                    continue;
                }
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                block_model_entries.push((short, bytes));
            }""",
    """            } else if name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json") {
                let short = name
                    .trim_end_matches(".json")
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                if short.is_empty() {
                    continue;
                }
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                block_model_entries.push((short, bytes));
            } else if name.starts_with(ITEM_MODEL_ROOT) && name.ends_with(".json") {
                let short = name
                    .trim_end_matches(".json")
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                if short.is_empty() {
                    continue;
                }
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                item_model_entries.push((short, bytes));
            } else if name.starts_with(ATLAS_ROOT) && name.ends_with(".json") {
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                atlas_entries.push((name, bytes));
            }""",
)

swap(
    """    Ok(Source {
        jobs,
        block_model_entries,
        pack_format,
    })
}

fn read_entry(""",
    """    Ok(Source {
        jobs,
        block_model_entries,
        item_model_entries,
        atlas_entries,
        pack_format,
    })
}

fn read_entry(""",
)

# The roots.
swap(
    """const BLOCK_MODEL_ROOT: &str = "assets/minecraft/models/block/";""",
    """const BLOCK_MODEL_ROOT: &str = "assets/minecraft/models/block/";
const ITEM_MODEL_ROOT: &str = "assets/minecraft/models/item/";
const ATLAS_ROOT: &str = "assets/minecraft/atlases/";""",
)

open(p, "w").write(s)
print("jar.rs reads the item models and the atlases")
