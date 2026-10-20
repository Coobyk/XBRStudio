#!/usr/bin/env python3
"""Print the CPU and the GPU values to find the difference."""
import sys

p = "src/gpu.rs"
s = open(p).read()

old = """    #[test]
    fn the_gpu_matches_the_cpu() {"""
new = """    #[test]
    fn debug_difference() {
        let Ok(context) = GpuContext::new() else {
            return;
        };
        let mut image = RgbaImage::new(16, 16);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 16) as u8, (y * 16) as u8, 64, 255]);
        }
        let gpu = context.upscale(&image, 4).unwrap();
        let cpu = crate::scalers::fsr::scale_fsr(&image, 4);
        for y in [0u32, 1, 2, 4, 8] {
            for x in [0u32, 1, 2, 4, 8, 17] {
                println!(
                    "({x},{y}) gpu {:?} cpu {:?}",
                    gpu.get_pixel(x, y).0,
                    cpu.get_pixel(x, y).0
                );
            }
        }
    }

    #[test]
    fn the_gpu_matches_the_cpu() {"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("debug test added")
