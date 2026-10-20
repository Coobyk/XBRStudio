#!/usr/bin/env python3
"""Fix the texture upload stride and the luma scale in the shader."""
import sys

p = "src/gpu.rs"
s = open(p).read()

old = """        let source_bytes = image.as_raw();
        let padded = {
            // The rows have to be a multiple of 256 bytes.
            let unpadded = width as usize * 4;
            let align = unpadded.div_ceil(256) * 256;
            let mut buffer = vec![0u8; align * height as usize];
            for row in 0..height as usize {
                let start = row * align;
                buffer[start..start + unpadded].copy_from_slice(&source_bytes[row * unpadded..(row + 1) * unpadded]);
            }
            buffer
        };"""
new = """        let source_bytes = image.as_raw();
        let unpadded = width as usize * 4;
        // The rows of a texture have to be a multiple of 256 bytes.
        let stride = unpadded.div_ceil(256) * 256;
        let padded = {
            let mut buffer = vec![0u8; stride * height as usize];
            for row in 0..height as usize {
                let start = row * stride;
                buffer[start..start + unpadded]
                    .copy_from_slice(&source_bytes[row * unpadded..(row + 1) * unpadded]);
            }
            buffer
        };"""
assert old in s
s = s.replace(old, new)

old = """                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),"""
new = """                bytes_per_row: Some(stride as u32),
                rows_per_image: Some(height),"""
assert old in s
s = s.replace(old, new)

open(p, "w").write(s)
print("gpu.rs upload fixed")

p = "src/gpu_shader.wgsl"
s = open(p).read()
old = """fn luma(pixel: vec4<f32>) -> f32 {
    return (0.2126 * pixel.r + 0.7152 * pixel.g + 0.0722 * pixel.b) * pixel.a;
}"""
new = """fn luma(pixel: vec4<f32>) -> f32 {
    // The luma is scaled to 0..255, like the CPU implementation.
    return (0.2126 * pixel.r + 0.7152 * pixel.g + 0.0722 * pixel.b) * pixel.a * 255.0;
}"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("shader luma fixed")
