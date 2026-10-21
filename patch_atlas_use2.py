#!/usr/bin/env python3
"""Collect the assets in the directory reader and use the atlas sprites."""
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
    """    let block_models = if opts.stitch {""",
    """    // The sprites of the atlases say which textures the game packs into an
    // atlas; the connected textures are only written for those.
    let atlas_sprites: std::collections::HashSet<String> =
        crate::assets::parse_atlases(&source.atlas_entries)
            .into_iter()
            .collect();
    let block_models = if opts.stitch {""",
)

swap(
    """    let block_models = if opts.stitch {
        let mut models = load_block_models(&source.block_model_entries);
        // The item models have the same layout, so they can be stitched too.
        for (stem, faces) in crate::assets::load_item_models(&source.item_model_entries) {
            models.entry(stem).or_insert(faces);
        }
        models
    } else {
        HashMap::new()
    };""",
    """    let block_models = if opts.stitch {
        let mut models = load_block_models(&source.block_model_entries);
        // The item models have the same layout, so they can be stitched too.
        for (stem, faces) in crate::assets::load_item_models(&source.item_model_entries) {
            models.entry(stem).or_insert(faces);
        }
        models
    } else {
        HashMap::new()
    };""",
)

open(p, "w").write(s)
print("directory reader collects the assets")
