#!/usr/bin/env python3
"""Guard the texture sampling against empty images."""
import sys

p = "src/ctm.rs"
s = open(p).read()

old = """fn sample_scaled(image: &RgbaImage, x: u32, y: u32, size: u32) -> Rgba<u8> {
    let sx = (x as u64 * image.width() as u64 / size as u64) as u32;
    let sy = (y as u64 * image.height() as u64 / size as u64) as u32;
    *image.get_pixel(sx.min(image.width() - 1), sy.min(image.height() - 1))
}"""
new = """fn sample_scaled(image: &RgbaImage, x: u32, y: u32, size: u32) -> Rgba<u8> {
    if image.width() == 0 || image.height() == 0 || size == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let sx = (x as u64 * image.width() as u64 / size as u64) as u32;
    let sy = (y as u64 * image.height() as u64 / size as u64) as u32;
    *image.get_pixel(
        sx.min(image.width() - 1),
        sy.min(image.height() - 1),
    )
}"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("sample_scaled is guarded")
