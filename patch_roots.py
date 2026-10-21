#!/usr/bin/env python3
"""Add the roots of the item models and the atlases."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """const BLOCK_MODEL_ROOT: &str = "assets/minecraft/models/block/";"""
new = """const BLOCK_MODEL_ROOT: &str = "assets/minecraft/models/block/";
const ITEM_MODEL_ROOT: &str = "assets/minecraft/models/item/";
const ATLAS_ROOT: &str = "assets/minecraft/atlases/";"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("roots added")
