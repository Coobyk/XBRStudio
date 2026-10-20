#!/usr/bin/env python3
"""Fix the connected-face test."""
import sys

p = "src/ctm.rs"
s = open(p).read()

old = """    #[test]
    fn a_fully_connected_face_uses_the_plain_tile() {
        assert_eq!(tile_index([true; 4], [true; 4]), 14);
        assert_eq!(tile_index([false; 4], [false; 4]), 0);
    }"""

new = """    #[test]
    fn a_fully_connected_face_uses_the_plain_tile() {
        // Everything connected and no corner connected: the plain face.
        assert_eq!(tile_index([true; 4], [false; 4]), 14);
        // Every corner connects as well, so the tile is refined.
        assert_eq!(tile_index([true; 4], [true; 4]), 7);
        // Nothing connects: the plain face as well.
        assert_eq!(tile_index([false; 4], [false; 4]), 0);
    }"""

assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("test fixed")
