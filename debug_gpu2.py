#!/usr/bin/env python3
"""Print the biggest differences between the GPU and the CPU."""
import sys

p = "src/gpu.rs"
s = open(p).read()

start = s.index("    #[test]\n    fn debug_difference() {")
end = s.index("    #[test]\n    fn the_gpu_matches_the_cpu() {")

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
        let mut worst: Vec<(i32, u32, u32, [u8; 4], [u8; 4])> = Vec::new();
        for y in 0..gpu.height() {
            for x in 0..gpu.width() {
                let a = gpu.get_pixel(x, y).0;
                let b = cpu.get_pixel(x, y).0;
                let difference = (0..4)
                    .map(|c| (a[c] as i32 - b[c] as i32).abs())
                    .max()
                    .unwrap();
                worst.push((difference, x, y, a, b));
            }
        }
        worst.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        for entry in worst.iter().take(12) {
            println!("diff {} at ({},{}) gpu {:?} cpu {:?}", entry.0, entry.1, entry.2, entry.3, entry.4);
        }
    }

"""

s = s[:start] + new + s[end:]
open(p, "w").write(s)
print("debug test updated")
