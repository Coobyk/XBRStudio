#!/usr/bin/env python3
"""Add the GPU test and the --gpu flag."""
import sys


def patch(path, pairs):
    with open(path, "r", encoding="utf-8") as handle:
        source = handle.read()
    for old, new in pairs:
        if old not in source:
            print("MISSING in %s:\n%s" % (path, old[:300]))
            sys.exit(1)
        if source.count(old) != 1:
            print("NOT UNIQUE in %s (%d):\n%s" % (path, source.count(old), old[:300]))
            sys.exit(1)
        source = source.replace(old, new)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(source)
    print("patched", path)


patch(
    "src/gpu.rs",
    [
        (
            """/// Scale an image with the GPU, falling back to the CPU when there is no
/// device.
pub fn upscale_or_fallback(""",
            """#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn the_gpu_matches_the_cpu() {
        let Ok(context) = GpuContext::new() else {
            // No device in this environment, nothing to compare.
            return;
        };

        let mut image = RgbaImage::new(16, 16);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 16) as u8, (y * 16) as u8, 64, 255]);
        }

        let gpu = context.upscale(&image, 4).unwrap();
        let cpu = crate::scalers::fsr::scale_fsr(&image, 4);
        assert_eq!(gpu.dimensions(), cpu.dimensions());

        let mut difference = 0u64;
        for (a, b) in gpu.pixels().zip(cpu.pixels()) {
            for channel in 0..4 {
                difference += (a.0[channel] as i32 - b.0[channel] as i32).unsigned_abs() as u64;
            }
        }
        let average = difference as f64 / (gpu.width() as f64 * gpu.height() as f64 * 4.0);
        assert!(average < 2.0, "the GPU and the CPU differ by {average}");
    }
}

/// Scale an image with the GPU, falling back to the CPU when there is no
/// device.
pub fn upscale_or_fallback(""",
        ),
    ],
)

patch(
    "src/main.rs",
    [
        (
            """    /// Extra sharpening after scaling (0.0-1.0).
    #[arg(long, default_value_t = 0.0)]
    sharpen: f32,""",
            """    /// Extra sharpening after scaling (0.0-1.0).
    #[arg(long, default_value_t = 0.0)]
    sharpen: f32,

    /// Use the GPU for the edge adaptive upscaler.
    #[arg(long)]
    gpu: bool,""",
        ),
        (
            """fn run(cli: Cli) -> Result<(), String> {
    let config = Config::load(cli.config.as_deref())?;""",
            """fn run(cli: Cli) -> Result<(), String> {
    let config = Config::load(cli.config.as_deref())?;

    if cli.gpu {
        #[cfg(feature = "gpu")]
        match xbrstudio::gpu::init() {
            Ok(()) => println!("using the GPU for the edge adaptive upscaler"),
            Err(error) => eprintln!("note: {error}"),
        }
        #[cfg(not(feature = "gpu"))]
        eprintln!("note: this build has no GPU backend");
    }""",
        ),
    ],
)
