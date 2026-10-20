#!/usr/bin/env python3
"""Use a 64x64 skin when the model has no texture size."""
import sys

p = "src/skin.rs"
s = open(p).read()

old = """    let parts = LegacyParts::classic();
    let mut output = RgbaImage::new(
        model.uv_size.0.round() as u32,
        model.uv_size.1.round() as u32,
    );"""
new = """    let parts = LegacyParts::classic();
    // Models without a texture size use the 16×16 block space; a skin is
    // always 64×64.
    let (uv_width, uv_height) = if model.uv_size.0 > 16.0 {
        (model.uv_size.0.round() as u32, model.uv_size.1.round() as u32)
    } else {
        (64, 64)
    };
    let mut output = RgbaImage::new(uv_width, uv_height);"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("skin.rs uses a 64x64 skin")
