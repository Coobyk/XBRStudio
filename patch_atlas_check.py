#!/usr/bin/env python3
"""Use the atlases in the batch."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """    // The sprites of the atlases say which textures the game packs into an
    // atlas; the connected textures are only written for those.
    let atlas_sprites: std::collections::HashSet<String> =
        crate::assets::parse_atlases(&source.atlas_entries)
            .into_iter()
            .collect();"""
new = """    // The atlases say which textures the game packs into an atlas; the
    // connected textures are only written for those.
    let atlases = crate::assets::parse_atlases(&source.atlas_entries);"""
assert old in s
s = s.replace(old, new)

old = """    atlas_sprites: &std::collections::HashSet<String>,
) -> Result<ProcessOutcome, String> {"""
new = """    atlases: &crate::assets::Atlases,
) -> Result<ProcessOutcome, String> {"""
assert old in s
s = s.replace(old, new)

s = s.replace("&atlas_sprites,", "&atlases,")

old = """        && (atlas_sprites.is_empty() || atlas_sprites.contains(&format!("block/{stem}.png")))"""
new = """        && atlases.contains(&format!("block/{stem}.png"))"""
assert old in s
s = s.replace(old, new)

open(p, "w").write(s)
print("jar.rs uses the atlases")
