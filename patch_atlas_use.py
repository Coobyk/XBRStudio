#!/usr/bin/env python3
"""Collect the assets in the directory reader and use the atlas sprites."""
import sys


def patch(path, pairs):
    with open(path, "r", encoding="utf-8") as handle:
        source = handle.read()
    for old, new in pairs:
        if old not in source:
            print("MISSING in %s:\n%s" % (path, old[:300]))
            sys.exit(1)
        if source.count(old) != 1:
            print("NOT UNIQUE in %s (%d):\n%s" % (path, source.count(old), old[:300]))
            sys.exit(1)
        source = source.replace(old, new)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(source)
    print("patched", path)


patch(
    "src/jar.rs",
    [
        # The directory reader collects the item models and the atlases.
        (
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
        ),
        # The sprites of the atlases.
        (
            """    let block_models = if opts.stitch {""",
            """    // The sprites of the atlases say which textures the game packs into
    // an atlas; the connected textures are only written for those.
    let atlas_sprites: std::collections::HashSet<String> = crate::assets::parse_atlases(
        &source.atlas_entries,
    )
    .into_iter()
    .collect();
    let block_models = if opts.stitch {""",
        ),
        # The batch uses them.
        (
            """            .map(|job| {
                let short = job""",
            """            .map(|job| {
                let short = job""",
        ),
    ],
)

# The atlas sprites reach the workers.
with open("src/jar.rs", encoding="utf-8") as handle:
    source = handle.read()

old = """            let outcome = match &cache {"""
new = """            let outcome = match &cache {"""
assert old in source

# The connected textures are only written for the sprites of the atlas.
old = """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
    {"""
new = """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
        && (atlas_sprites.is_empty() || atlas_sprites.contains(&format!("block/{stem}.png")))
    {"""
assert old in source
source = source.replace(old, new)

with open("src/jar.rs", "w", encoding="utf-8") as handle:
    handle.write(source)
print("atlas sprites used")
