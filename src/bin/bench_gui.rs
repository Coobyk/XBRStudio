//! A temporary benchmark of the work the desktop app does on the main thread.

use std::time::Instant;

use image::{Rgba, RgbaImage};
use xbrstudio::preview3d;
use xbrstudio::upscaling::{UpscaleConfig, upscale_image};

fn sample_nearest(image: &RgbaImage, x: u32, y: u32, width: u32, height: u32) -> Rgba<u8> {
    let sx = (x as u64 * image.width() as u64 / width as u64) as u32;
    let sy = (y as u64 * image.height() as u64 / height as u64) as u32;
    *image.get_pixel(sx.min(image.width() - 1), sy.min(image.height() - 1))
}

fn main() {
    let config = UpscaleConfig::default();

    for size in [16u32, 64, 256] {
        let mut image = RgbaImage::new(size, size);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x % 251) as u8, (y % 253) as u8, 90, 255]);
        }

        let start = Instant::now();
        let result = upscale_image(&image, None, &config).unwrap();
        let upscale = start.elapsed();

        // The comparison image: one pixel at a time.
        let width = result.width();
        let height = result.height();
        let start = Instant::now();
        let mut compare = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                compare.put_pixel(x, y, sample_nearest(&image, x, y, width, height));
            }
        }
        let compare = start.elapsed();

        println!(
            "texture {size}x{size} -> {width}x{height}: upscale {upscale:?}, compare {compare:?}"
        );
    }

    // The 3D preview.
    let mut image = RgbaImage::new(64, 64);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([(x * 4) as u8, (y * 4) as u8, 120, 255]);
    }
    let faces = xbrstudio::model::load_model(std::path::Path::new(
        "/data/workspace/XBRStudio/models/entity/CowModel.json",
    ))
    .unwrap();
    let mesh = preview3d::build_mesh(&faces);
    let start = Instant::now();
    let _ = preview3d::render(&mesh, &image, 320, 320, 0.6, 0.3);
    println!("3d preview: {:?}", start.elapsed());
}
