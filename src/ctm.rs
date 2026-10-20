//! OptiFine connected textures.
//!
//! OptiFine can blend a block's texture with its neighbours at runtime. It
//! picks one of 47 tiles depending on which of the four sides and which of
//! the four corners connect, so a pack has to ship 48 images (the 48th is
//! unused) plus a properties file.
//!
//! This module reproduces OptiFine's tile selection and composes the tiles
//! from the block's own faces: a quadrant that touches a side which does
//! *not* connect is taken from the face on that side, which gives the
//! familiar "grass on top, dirt on the side" look.

use std::collections::HashMap;
use std::path::Path;

use image::{Rgba, RgbaImage};

/// The number of tiles OptiFine's `ctm` method uses.
pub const CTM_TILES: usize = 47;
/// The tiles are laid out with 16 per row.
pub const TILES_PER_ROW: usize = 16;

/// OptiFine's tile selection.
///
/// `borders` says which of the four sides connect, `edges` which of the four
/// corners connect. The result is the index of the tile to use.
pub fn tile_index(borders: [bool; 4], edges: [bool; 4]) -> usize {
    let b = |i: usize| borders[i];
    let e = |i: usize| edges[i];

    let mut index = 0usize;
    if b(0) && !b(1) && !b(2) && !b(3) {
        index = 3;
    } else if !b(0) && b(1) && !b(2) && !b(3) {
        index = 1;
    } else if !b(0) && !b(1) && b(2) && !b(3) {
        index = 12;
    } else if !b(0) && !b(1) && !b(2) && b(3) {
        index = 36;
    } else if b(0) && b(1) && !b(2) && !b(3) {
        index = 2;
    } else if !b(0) && !b(1) && b(2) && b(3) {
        index = 24;
    } else if b(0) && !b(1) && b(2) && !b(3) {
        index = 15;
    } else if b(0) && !b(1) && !b(2) && b(3) {
        index = 39;
    } else if b(0) && b(1) && !b(2) && b(3) {
        index = 37;
    } else if b(0) && b(1) && b(2) && !b(3) {
        index = 13;
    } else if b(0) && b(1) && b(2) && b(3) {
        index = 14;
    } else if !b(0) && b(1) && b(2) && b(3) {
        index = 25;
    } else if b(0) && !b(1) && b(2) && b(3) {
        index = 27;
    }

    // The "fancy" variants use the corners as well.
    if index == 13 && e(0) {
        index = 4;
    } else if index == 15 && e(1) {
        index = 5;
    } else if index == 37 && e(2) {
        index = 16;
    } else if index == 39 && e(3) {
        index = 17;
    } else if index == 14 && e(0) && e(1) {
        index = 7;
    } else if index == 25 && e(0) && e(2) {
        index = 6;
    } else if index == 27 && e(3) && e(1) {
        index = 19;
    } else if index == 38 && e(3) && e(2) {
        index = 18;
    } else if index == 14 && !e(0) && e(1) {
        index = 31;
    } else if index == 25 && e(0) && !e(2) {
        index = 30;
    } else if index == 27 && !e(3) && e(1) {
        index = 41;
    } else if index == 38 && e(3) && !e(2) {
        index = 40;
    } else if index == 14 && e(0) && !e(1) {
        index = 29;
    } else if index == 25 && !e(0) && e(2) {
        index = 28;
    } else if index == 27 && e(3) && !e(1) {
        index = 43;
    } else if index == 38 && !e(3) && e(2) {
        index = 42;
    } else if index == 26 && e(0) && e(1) && e(2) && e(3) {
        index = 46;
    } else if index == 26 && !e(0) && e(1) && e(2) && e(3) {
        index = 9;
    } else if index == 26 && e(0) && !e(1) && e(2) && e(3) {
        index = 21;
    } else if index == 26 && e(0) && e(1) && !e(2) && e(3) {
        index = 8;
    } else if index == 26 && e(0) && e(1) && e(2) && !e(3) {
        index = 20;
    } else if index == 26 && e(0) && e(1) && !e(2) && !e(3) {
        index = 11;
    } else if index == 26 && !e(0) && !e(1) && e(2) && e(3) {
        index = 22;
    } else if index == 26 && !e(0) && e(1) && !e(2) && e(3) {
        index = 23;
    } else if index == 26 && e(0) && !e(1) && !e(2) && e(3) {
        index = 10;
    } else if index == 26 && !e(0) && !e(1) && !e(2) && e(3) {
        index = 34;
    } else if index == 26 && !e(0) && !e(1) && e(2) && !e(3) {
        index = 35;
    } else if index == 26 && e(0) && !e(1) && !e(2) && !e(3) {
        index = 32;
    } else if index == 26 && !e(0) && e(1) && e(2) && !e(3) {
        index = 45;
    } else if index == 26 && e(0) && e(1) && !e(2) && e(3) {
        index = 44;
    }

    index
}

/// The faces of a block, keyed by their name.
#[derive(Debug, Clone, Default)]
pub struct BlockFaces {
    pub faces: HashMap<String, RgbaImage>,
}

impl BlockFaces {
    pub fn get(&self, name: &str) -> Option<&RgbaImage> {
        self.faces.get(name)
    }

    pub fn insert(&mut self, name: &str, image: RgbaImage) {
        self.faces.insert(name.to_string(), image);
    }
}

/// The tile sheet of one block: 48 images, 16 per row.
pub fn compose_tiles(block: &BlockFaces) -> Vec<RgbaImage> {
    let face = block
        .get("north")
        .or_else(|| block.get("south"))
        .or_else(|| block.get("east"))
        .or_else(|| block.get("west"))
        .cloned()
        .unwrap_or_else(|| RgbaImage::new(16, 16));

    let size = face.width().min(face.height());
    let half = size / 2;

    // The patterns that lead to every tile. The first one found wins, so the
    // tiles are stable.
    let mut patterns: Vec<([bool; 4], [bool; 4])> = Vec::new();
    for borders in 0..16usize {
        for edges in 0..16usize {
            let border_flags = [
                borders & 1 != 0,
                borders & 2 != 0,
                borders & 4 != 0,
                borders & 8 != 0,
            ];
            let edge_flags = [
                edges & 1 != 0,
                edges & 2 != 0,
                edges & 4 != 0,
                edges & 8 != 0,
            ];
            patterns.push((border_flags, edge_flags));
        }
    }

    let mut tiles: Vec<Option<RgbaImage>> = vec![None; CTM_TILES];
    for (borders, edges) in patterns {
        let index = tile_index(borders, edges);
        if index < CTM_TILES && tiles[index].is_none() {
            tiles[index] = Some(compose_tile(block, &face, borders, size, half));
        }
    }

    // Tiles no pattern reached are filled with the plain face.
    tiles
        .into_iter()
        .map(|tile| tile.unwrap_or_else(|| face.clone()))
        .collect()
}

/// Compose one tile from the block's faces.
///
/// The reference face is the north face. In its image the east side is on
/// the left, the west side on the right, down at the bottom and up at the
/// top. A quadrant that touches a side which does not connect is taken from
/// the face on that side.
fn compose_tile(
    block: &BlockFaces,
    face: &RgbaImage,
    borders: [bool; 4],
    size: u32,
    half: u32,
) -> RgbaImage {
    let mut tile = RgbaImage::new(size, size);

    // The face that replaces a quadrant, per side: east, west, down, up.
    let side_faces = ["east", "west", "down", "up"];

    for y in 0..size {
        for x in 0..size {
            let top = y < half;
            let left = x < half;

            // The side the quadrant touches horizontally and vertically.
            let horizontal = if left { 0usize } else { 1usize };
            let vertical = if top { 3usize } else { 2usize };

            let source = if !borders[horizontal] {
                block.get(side_faces[horizontal])
            } else if !borders[vertical] {
                block.get(side_faces[vertical])
            } else {
                None
            };

            let pixel = match source {
                Some(source) => sample_scaled(source, x, y, size),
                None => *face.get_pixel(x.min(face.width() - 1), y.min(face.height() - 1)),
            };
            tile.put_pixel(x, y, pixel);
        }
    }

    tile
}

/// Read a pixel of an image, scaled to the tile size.
fn sample_scaled(image: &RgbaImage, x: u32, y: u32, size: u32) -> Rgba<u8> {
    let sx = (x as u64 * image.width() as u64 / size as u64) as u32;
    let sy = (y as u64 * image.height() as u64 / size as u64) as u32;
    *image.get_pixel(sx.min(image.width() - 1), sy.min(image.height() - 1))
}

/// The folders OptiFine looks in for connected textures.
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
    let tile_names: Vec<String> = (0..tiles.len())
        .map(|index| format!("{name}_{index}"))
        .collect();

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
            "method=ctm\nmatch={match_texture}\ntiles={}\n",
            tile_names.join(" ")
        );
        let path = dir.join(format!("{name}.properties"));
        std::fs::write(&path, properties)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fully_connected_face_uses_the_plain_tile() {
        // Everything connected and no corner connected: the plain face.
        assert_eq!(tile_index([true; 4], [false; 4]), 14);
        // Every corner connects as well, so the tile is refined.
        assert_eq!(tile_index([true; 4], [true; 4]), 7);
        // Nothing connects: the plain face as well.
        assert_eq!(tile_index([false; 4], [false; 4]), 0);
    }

    #[test]
    fn single_sides_match_optifine() {
        assert_eq!(tile_index([true, false, false, false], [false; 4]), 3);
        assert_eq!(tile_index([false, true, false, false], [false; 4]), 1);
        assert_eq!(tile_index([false, false, true, false], [false; 4]), 12);
        assert_eq!(tile_index([false, false, false, true], [false; 4]), 36);
    }

    #[test]
    fn two_sides_match_optifine() {
        assert_eq!(tile_index([true, true, false, false], [false; 4]), 2);
        assert_eq!(tile_index([true, false, true, false], [false; 4]), 15);
        assert_eq!(tile_index([true, false, false, true], [false; 4]), 39);
        assert_eq!(tile_index([false, false, true, true], [false; 4]), 24);
    }

    #[test]
    fn three_sides_match_optifine() {
        assert_eq!(tile_index([true, true, true, false], [false; 4]), 13);
        assert_eq!(tile_index([true, true, false, true], [false; 4]), 37);
        assert_eq!(tile_index([true, false, true, true], [false; 4]), 27);
        assert_eq!(tile_index([false, true, true, true], [false; 4]), 25);
    }

    #[test]
    fn uncovered_patterns_use_the_plain_tile() {
        // The west/down and west/up pairs are not part of OptiFine's table.
        assert_eq!(tile_index([false, true, true, false], [false; 4]), 0);
        assert_eq!(tile_index([false, true, false, true], [false; 4]), 0);
    }

    #[test]
    fn corners_refine_the_tile() {
        assert_eq!(
            tile_index([true, true, true, false], [true, false, false, false]),
            4
        );
        assert_eq!(
            tile_index([true, false, true, false], [false, true, false, false]),
            5
        );
    }

    #[test]
    fn every_index_is_inside_the_sheet() {
        for borders in 0..16usize {
            for edges in 0..16usize {
                let border_flags = [
                    borders & 1 != 0,
                    borders & 2 != 0,
                    borders & 4 != 0,
                    borders & 8 != 0,
                ];
                let edge_flags = [
                    edges & 1 != 0,
                    edges & 2 != 0,
                    edges & 4 != 0,
                    edges & 8 != 0,
                ];
                let index = tile_index(border_flags, edge_flags);
                assert!(index < CTM_TILES, "index {index} is out of range");
            }
        }
    }

    #[test]
    fn composes_a_sheet_of_47_tiles() {
        let mut block = BlockFaces::default();
        block.insert(
            "north",
            RgbaImage::from_pixel(16, 16, Rgba([10, 10, 10, 255])),
        );
        block.insert(
            "south",
            RgbaImage::from_pixel(16, 16, Rgba([20, 20, 20, 255])),
        );
        block.insert(
            "east",
            RgbaImage::from_pixel(16, 16, Rgba([30, 30, 30, 255])),
        );
        block.insert(
            "west",
            RgbaImage::from_pixel(16, 16, Rgba([40, 40, 40, 255])),
        );
        block.insert("up", RgbaImage::from_pixel(16, 16, Rgba([50, 50, 50, 255])));
        block.insert(
            "down",
            RgbaImage::from_pixel(16, 16, Rgba([60, 60, 60, 255])),
        );

        let tiles = compose_tiles(&block);
        assert_eq!(tiles.len(), CTM_TILES);
        for tile in &tiles {
            assert_eq!(tile.dimensions(), (16, 16));
        }
    }
}
