#!/usr/bin/env python3
"""Rewrite the tile index function to match OptiFine exactly and fix the tests."""
import sys

p = "src/ctm.rs"
s = open(p).read()

old_start = s.index("pub fn tile_index(borders: [bool; 4], edges: [bool; 4]) -> usize {")
old_end = s.index("/// The faces of a block, keyed by their name.")

new_fn = '''pub fn tile_index(borders: [bool; 4], edges: [bool; 4]) -> usize {
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

'''

s = s[:old_start] + new_fn + s[old_end:]

# Replace the tile composition with a clear one.
old_compose_start = s.index("/// Compose one tile from the block's faces.")
old_compose_end = s.index("/// Read a pixel of an image, scaled to the tile size.")

new_compose = '''/// Compose one tile from the block's faces.
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

'''

s = s[:old_compose_start] + new_compose + s[old_compose_end:]

# Fix the tests that assumed a different mapping.
old_test = """    #[test]
    fn two_sides_match_optifine() {
        assert_eq!(tile_index([true, true, false, false], [false; 4]), 2);
        assert_eq!(tile_index([true, false, true, false], [false; 4]), 15);
        assert_eq!(tile_index([true, false, false, true], [false; 4]), 39);
        assert_eq!(tile_index([false, true, true, false], [false; 4]), 24);
        assert_eq!(tile_index([false, true, false, true], [false; 4]), 25);
        assert_eq!(tile_index([false, false, true, true], [false; 4]), 37);
    }"""
new_test = """    #[test]
    fn two_sides_match_optifine() {
        assert_eq!(tile_index([true, true, false, false], [false; 4]), 2);
        assert_eq!(tile_index([true, false, true, false], [false; 4]), 15);
        assert_eq!(tile_index([true, false, false, true], [false; 4]), 39);
        assert_eq!(tile_index([false, false, true, true], [false; 4]), 24);
    }"""
assert old_test in s
s = s.replace(old_test, new_test)

old_three = """    #[test]
    fn three_sides_match_optifine() {
        assert_eq!(tile_index([true, true, true, false], [false; 4]), 13);
        assert_eq!(tile_index([true, true, false, true], [false; 4]), 37);
        assert_eq!(tile_index([true, false, true, true], [false; 4]), 27);
        assert_eq!(tile_index([false, true, true, true], [false; 4]), 25);
    }"""
new_three = """    #[test]
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
    }"""
assert old_three in s
s = s.replace(old_three, new_three)

open(p, "w").write(s)
print("ctm.rs rewritten")
