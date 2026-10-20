//! Separable windowed resampling.
//!
//! Every filter here is evaluated in *destination* space, so a Lanczos-3
//! kernel spans three output pixels rather than three source pixels. That
//! matters at x4: the classic source-space formulation blurs a 16×16 texture
//! into mush, while the destination-space one keeps the pixel art readable.

use image::{Rgba, RgbaImage};

use super::AlphaMode;

/// A separable windowed filter.
pub trait Kernel: Sync {
    /// Filter support in destination pixels.
    fn support(&self) -> f32;
    /// Filter value at `x` destination pixels from the centre.
    fn value(&self, x: f32) -> f32;
}

/// Lanczos-3: `sinc(x) * sinc(x/3)` over `|x| < 3`.
pub struct Lanczos3;

impl Kernel for Lanczos3 {
    fn support(&self) -> f32 {
        3.0
    }

    fn value(&self, x: f32) -> f32 {
        if x == 0.0 {
            return 1.0;
        }
        if x.abs() >= 3.0 {
            return 0.0;
        }
        let pix = std::f32::consts::PI * x;
        let sinc = pix.sin() / pix;
        let pix3 = pix / 3.0;
        let sinc3 = pix3.sin() / pix3;
        sinc * sinc3
    }
}

/// Catmull-Rom cubic (a = -0.5), support 2.
pub struct CatmullRom;

impl Kernel for CatmullRom {
    fn support(&self) -> f32 {
        2.0
    }

    fn value(&self, x: f32) -> f32 {
        let x = x.abs();
        if x < 1.0 {
            1.5 * x * x * x - 2.5 * x * x + 1.0
        } else if x < 2.0 {
            -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
        } else {
            0.0
        }
    }
}

/// How the resampler treats the alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaHandling {
    Straight,
    Premultiplied,
    Masked,
}

impl From<AlphaMode> for AlphaHandling {
    fn from(mode: AlphaMode) -> Self {
        match mode {
            AlphaMode::Straight => Self::Straight,
            AlphaMode::Premultiplied => Self::Premultiplied,
            AlphaMode::Masked => Self::Masked,
        }
    }
}

/// Per-axis tap table: for every destination index the source indices that
/// contribute and with which weight.
struct AxisTaps {
    taps: Vec<Vec<(usize, f32)>>,
}

fn build_axis_taps(src_len: u32, dst_len: u32, kernel: &dyn Kernel) -> AxisTaps {
    let scale = dst_len as f32 / src_len as f32;
    let support = kernel.support() / scale;
    let mut taps = Vec::with_capacity(dst_len as usize);

    for index in 0..dst_len {
        let center = (index as f32 + 0.5) / scale - 0.5;
        let lo = (center - support).ceil() as i64;
        let hi = (center + support).floor() as i64;
        let mut row: Vec<(usize, f32)> = Vec::new();
        let mut total = 0.0f32;
        for source in lo..=hi {
            if source < 0 || source >= src_len as i64 {
                continue;
            }
            let distance = (source as f32 - center) * scale;
            let weight = kernel.value(distance);
            if weight == 0.0 {
                continue;
            }
            total += weight;
            row.push((source as usize, weight));
        }
        if row.is_empty() {
            // Degenerate fallback: nearest source pixel.
            let clamped = center.round().clamp(0.0, src_len as f32 - 1.0) as usize;
            row.push((clamped, 1.0));
        } else if (total - 1.0).abs() > 1e-5 {
            for entry in row.iter_mut() {
                entry.1 /= total;
            }
        }
        taps.push(row);
    }

    AxisTaps { taps }
}

/// Nearest neighbour upscale.
pub fn scale_nearest(image: &RgbaImage, factor: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let mut out = RgbaImage::new(width * factor, height * factor);
    for y in 0..height * factor {
        let source_y = y / factor;
        for x in 0..width * factor {
            let source_x = x / factor;
            out.put_pixel(x, y, *image.get_pixel(source_x, source_y));
        }
    }
    out
}

/// Scale with a separable windowed kernel.
pub fn scale_windowed(
    image: &RgbaImage,
    factor: u32,
    kernel: &dyn Kernel,
    handling: AlphaHandling,
) -> RgbaImage {
    let (width, height) = image.dimensions();
    let out_width = width * factor;
    let out_height = height * factor;

    let source = image.as_raw();
    let mut pixels = vec![[0.0f32; 4]; (width as usize) * (height as usize)];
    for (index, chunk) in source.chunks_exact(4).enumerate() {
        let alpha = chunk[3] as f32 / 255.0;
        let (r, g, b) = match handling {
            AlphaHandling::Premultiplied => (
                chunk[0] as f32 * alpha,
                chunk[1] as f32 * alpha,
                chunk[2] as f32 * alpha,
            ),
            _ => (chunk[0] as f32, chunk[1] as f32, chunk[2] as f32),
        };
        pixels[index] = [r, g, b, chunk[3] as f32];
    }

    let x_taps = build_axis_taps(width, out_width, kernel);
    let y_taps = build_axis_taps(height, out_height, kernel);

    match handling {
        AlphaHandling::Masked => {
            scale_masked(image, &x_taps, &y_taps, out_width, out_height)
        }
        _ => {
            // Horizontal pass into an intermediate buffer of out_width × height.
            let mut intermediate = vec![[0.0f32; 4]; (out_width as usize) * (height as usize)];
            for y in 0..height as usize {
                for x in 0..out_width as usize {
                    let mut acc = [0.0f32; 4];
                    for (source_x, weight) in &x_taps.taps[x] {
                        let source = pixels[y * width as usize + *source_x];
                        for channel in 0..4 {
                            acc[channel] += source[channel] * *weight;
                        }
                    }
                    intermediate[y * out_width as usize + x] = acc;
                }
            }

            let mut out = RgbaImage::new(out_width, out_height);
            for y in 0..out_height as usize {
                for x in 0..out_width as usize {
                    let mut acc = [0.0f32; 4];
                    for (source_y, weight) in &y_taps.taps[y] {
                        let source = intermediate[*source_y * out_width as usize + x];
                        for channel in 0..4 {
                            acc[channel] += source[channel] * *weight;
                        }
                    }
                    let alpha = (acc[3].clamp(0.0, 255.0) / 255.0).max(f32::MIN_POSITIVE);
                    let pixel = match handling {
                        AlphaHandling::Premultiplied => Rgba([
                            (acc[0] / alpha).round().clamp(0.0, 255.0) as u8,
                            (acc[1] / alpha).round().clamp(0.0, 255.0) as u8,
                            (acc[2] / alpha).round().clamp(0.0, 255.0) as u8,
                            acc[3].round().clamp(0.0, 255.0) as u8,
                        ]),
                        _ => Rgba([
                            acc[0].round().clamp(0.0, 255.0) as u8,
                            acc[1].round().clamp(0.0, 255.0) as u8,
                            acc[2].round().clamp(0.0, 255.0) as u8,
                            acc[3].round().clamp(0.0, 255.0) as u8,
                        ]),
                    };
                    out.put_pixel(x as u32, y as u32, pixel);
                }
            }
            out
        }
    }
}

/// Alpha-weighted two-pass resampling: a tap only contributes if the texel
/// it reads is (partly) opaque, so fully transparent texels never bleed
/// their colour into the result.
#[allow(clippy::too_many_arguments)]
fn scale_masked(
    image: &RgbaImage,
    x_taps: &AxisTaps,
    y_taps: &AxisTaps,
    out_width: u32,
    out_height: u32,
) -> RgbaImage {
    let mut out = RgbaImage::new(out_width, out_height);
    for y in 0..out_height as usize {
        for x in 0..out_width as usize {
            let mut rgb = [0.0f32; 3];
            let mut rgb_weight = 0.0f32;
            let mut alpha = 0.0f32;
            let mut alpha_weight = 0.0f32;

            for (source_y, weight_y) in &y_taps.taps[y] {
                for (source_x, weight_x) in &x_taps.taps[x] {
                    let weight = weight_x * weight_y;
                    let source = *image.get_pixel(*source_x as u32, *source_y as u32);
                    let tap_alpha = source.0[3] as f32 / 255.0;
                    let masked = weight * tap_alpha;
                    rgb[0] += source.0[0] as f32 * masked;
                    rgb[1] += source.0[1] as f32 * masked;
                    rgb[2] += source.0[2] as f32 * masked;
                    rgb_weight += masked;
                    alpha += source.0[3] as f32 * weight;
                    alpha_weight += weight;
                }
            }

            // A ringing filter can produce negative weights; the numerator
            // carries the same sign, so the ratio stays positive.
            let pixel = if rgb_weight.abs() > 1e-6 {
                Rgba([
                    (rgb[0] / rgb_weight).round().clamp(0.0, 255.0) as u8,
                    (rgb[1] / rgb_weight).round().clamp(0.0, 255.0) as u8,
                    (rgb[2] / rgb_weight).round().clamp(0.0, 255.0) as u8,
                    (alpha / alpha_weight).round().clamp(0.0, 255.0) as u8,
                ])
            } else {
                Rgba([0, 0, 0, (alpha / alpha_weight).round().clamp(0.0, 255.0) as u8])
            };
            out.put_pixel(x as u32, y as u32, pixel);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32) -> RgbaImage {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 16) as u8, (y * 16) as u8, 0, 255]);
        }
        image
    }

    #[test]
    fn nearest_doubles_pixels_exactly() {
        let source = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 4]));
        let scaled = scale_nearest(&source, 3);
        assert_eq!(scaled.dimensions(), (6, 6));
        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(scaled.get_pixel(x, y).0, [1, 2, 3, 4]);
            }
        }
    }

    #[test]
    fn lanczos_preserves_a_constant_image() {
        let source = RgbaImage::from_pixel(4, 4, Rgba([90, 80, 70, 255]));
        let scaled = scale_windowed(&source, 4, &Lanczos3, AlphaHandling::Straight);
        for pixel in scaled.pixels() {
            assert_eq!(pixel.0, [90, 80, 70, 255]);
        }
    }

    #[test]
    fn bicubic_preserves_a_constant_image() {
        let source = RgbaImage::from_pixel(4, 4, Rgba([5, 6, 7, 255]));
        let scaled = scale_windowed(&source, 2, &CatmullRom, AlphaHandling::Straight);
        for pixel in scaled.pixels() {
            assert_eq!(pixel.0, [5, 6, 7, 255]);
        }
    }

    /// A fully transparent green texel next to an opaque red one.
    fn transparent_colour_pair() -> RgbaImage {
        let mut source = RgbaImage::new(2, 1);
        source.put_pixel(0, 0, Rgba([0, 255, 0, 0]));
        source.put_pixel(1, 0, Rgba([255, 0, 0, 255]));
        source
    }

    #[test]
    fn premultiplied_keeps_transparent_colors_out_of_the_result() {
        let source = transparent_colour_pair();

        let straight = scale_windowed(&source, 2, &CatmullRom, AlphaHandling::Straight);
        let premultiplied = scale_windowed(&source, 2, &CatmullRom, AlphaHandling::Premultiplied);

        // Pixel 1 of 4 is the one that mixes the transparent texel with the
        // opaque one.
        assert!(
            straight.get_pixel(1, 0).0[1] > 0,
            "straight interpolation is expected to bleed the transparent colour"
        );
        assert_eq!(
            premultiplied.get_pixel(1, 0).0[1],
            0,
            "premultiplied interpolation must not bleed the transparent colour"
        );
    }

    #[test]
    fn masked_ignores_transparent_texels() {
        let source = transparent_colour_pair();

        let masked = scale_windowed(&source, 2, &CatmullRom, AlphaHandling::Masked);
        let pixel = masked.get_pixel(1, 0).0;
        assert_eq!(pixel[0], 255, "masked resampling lost the opaque colour");
        assert_eq!(pixel[1], 0, "masked resampling leaked the transparent colour");
    }

    #[test]
    fn gradient_scales_without_shifting() {
        let source = gradient(8, 8);
        let scaled = scale_windowed(&source, 2, &Lanczos3, AlphaHandling::Straight);
        // Middle of the gradient: the red channel grows with x.
        let left = scaled.get_pixel(8, 8).0[0];
        let right = scaled.get_pixel(10, 8).0[0];
        assert!(right > left, "gradient was not preserved");
    }
}
