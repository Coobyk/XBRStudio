#!/usr/bin/env python3
"""Wire connected textures into the batch."""
import sys

p = "src/ctm.rs"
s = open(p).read()

old = """/// Write the connected textures of a block: the properties file and the
/// tile images.
pub fn write_block(
    out_dir: &Path,
    name: &str,
    match_texture: &str,
    tiles: &[RgbaImage],
) -> Result<(), String> {
    let dir = out_dir.join("assets/minecraft/ctm/blocks").join(name);
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;

    for (index, tile) in tiles.iter().enumerate() {
        let path = dir.join(format!("{index}.png"));
        tile.save(&path)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }

    let properties = format!(
        "method=ctm\\nmatch={match_texture}\\ntiles=0-{}\\n",
        CTM_TILES - 1
    );
    let path = dir.join(format!("{name}.properties"));
    std::fs::write(&path, properties)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(())
}"""

new = """/// The folders OptiFine looks in for connected textures.
pub const CTM_FOLDERS: [&str; 2] = [
    "assets/minecraft/optifine/ctm/blocks",
    "assets/minecraft/ctm/blocks",
];

/// Write the connected textures of a block: the properties file and the
/// tile images.
///
/// The tiles are written next to the properties file, prefixed with the
/// block name so several blocks can share the folder, and the properties
/// file lists them one by one.
pub fn write_block(
    out_dir: &Path,
    name: &str,
    match_texture: &str,
    tiles: &[RgbaImage],
) -> Result<(), String> {
    let tile_names: Vec<String> = (0..tiles.len()).map(|index| format!("{name}_{index}")).collect();

    for folder in CTM_FOLDERS {
        let dir = out_dir.join(folder);
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;

        for (index, tile) in tiles.iter().enumerate() {
            let path = dir.join(format!("{name}_{index}.png"));
            tile.save(&path)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        }

        let properties = format!(
            "method=ctm\\nmatch={match_texture}\\ntiles={}\\n",
            tile_names.join(" ")
        );
        let path = dir.join(format!("{name}.properties"));
        std::fs::write(&path, properties)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(())
}"""

assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("ctm writer updated")
