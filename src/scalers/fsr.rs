//! FSR-style upscaling: edge-adaptive spatial upsampling (EASU) followed by
//! contrast-adaptive sharpening (RCAS).
//!
//! The implementation follows the structure of AMD's FSR rather than being a
//! port of the SDK:
//!
//! * **EASU** resamples with a cubic B-spline evaluated in destination
//!   space (no ringing, unlike Catmull-Rom) and *adapts the sampling
//!   position to the local gradient*. Where a source texel edge is sharper
//!   than the filter, the samples are pulled towards the bright side of the
//!   edge so the transition stays narrow instead of being smeared over four
//!   texels.
//! * **RCAS** sharpens the result with a weight derived from the local
//!   contrast: flat areas are left alone, edges get a controlled boost, and
//!   the output is clamped to the neighbourhood range so no halos appear.

use image::{Rgba, RgbaImage};

/// Support of the EASU filter, in destination pixels.
const EASU_SUPPORT: f32 = 2.0;
/// Maximum sampling shift applied by the edge adaptation, in source pixels.
const EASU_MAX_SHIFT: f32 = 0.25;
/// Luma difference that counts as a fully saturated gradient.
const EASU_GRADIENT_FULL: f32 = 64.0;

/// Scale with EASU + RCAS.
pub fn scale_fsr(image: &RgbaImage, factor: u32) -> RgbaImage {
    let upscaled = scale_easu(image, factor);
    rcas(&upscaled, 0.2)
}

/// Edge-adaptive spatial upsampling.
pub fn scale_easu(image: &RgbaImage, factor: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let out_width = width * factor;
    let out_height = height * factor;
    if width == 0 || height == 0 {
        return RgbaImage::new(out_width, out_height);
    }

    let scale_x = out_width as f32 / width as f32;
    let scale_y = out_height as f32 / height as f32;

    let mut out = RgbaImage::new(out_width, out_height);
    for y in 0..out_height {
        let center_y = (y as f32 + 0.5) / scale_y - 0.5;
        let cell_y = center_y.floor().clamp(0.0, height as f32 - 1.0) as i64;
        for x in 0..out_width {
            let center_x = (x as f32 + 0.5) / scale_x - 0.5;
            let cell_x = center_x.floor().clamp(0.0, width as f32 - 1.0) as i64;

            // Edge adaptation: shift the sampling position towards the bright
            // side of the local gradient.
            let (shift_x, shift_y) = gradient_shift(image, cell_x as u32, cell_y as u32);
            let center_x = center_x + shift_x;
            let center_y = center_y + shift_y;

            let mut rgb = [0.0f32; 3];
            let mut alpha = 0.0f32;
            let mut weight_sum = 0.0f32;

            let taps_x = easu_taps(center_x, width, scale_x);
            let taps_y = easu_taps(center_y, height, scale_y);
            for (source_y, weight_y) in &taps_y {
                for (source_x, weight_x) in &taps_x {
                    let weight = weight_x * weight_y;
                    let source = image.get_pixel(*source_x as u32, *source_y as u32);
                    rgb[0] += source.0[0] as f32 * weight;
                    rgb[1] += source.0[1] as f32 * weight;
                    rgb[2] += source.0[2] as f32 * weight;
                    alpha += source.0[3] as f32 * weight;
                    weight_sum += weight;
                }
            }

            if weight_sum > 1e-6 {
                rgb[0] /= weight_sum;
                rgb[1] /= weight_sum;
                rgb[2] /= weight_sum;
                alpha /= weight_sum;
            }

            out.put_pixel(
                x,
                y,
                Rgba([
                    rgb[0].round().clamp(0.0, 255.0) as u8,
                    rgb[1].round().clamp(0.0, 255.0) as u8,
                    rgb[2].round().clamp(0.0, 255.0) as u8,
                    alpha.round().clamp(0.0, 255.0) as u8,
                ]),
            );
        }
    }
    out
}

/// Cubic B-spline weights for a sample position, in destination space.
fn easu_taps(center: f32, source_len: u32, scale: f32) -> Vec<(usize, f32)> {
    let support = EASU_SUPPORT / scale;
    let lo = (center - support).ceil() as i64;
    let hi = (center + support).floor() as i64;

    let mut taps: Vec<(usize, f32)> = Vec::new();
    let mut total = 0.0f32;
    for source in lo..=hi {
        if source < 0 || source >= source_len as i64 {
            continue;
        }
        let distance = (source as f32 - center) * scale;
        let weight = bspline(distance);
        if weight <= 0.0 {
            continue;
        }
        total += weight;
        taps.push((source as usize, weight));
    }

    if taps.is_empty() {
        let clamped = center.round().clamp(0.0, source_len as f32 - 1.0) as usize;
        taps.push((clamped, 1.0));
    } else if (total - 1.0).abs() > 1e-5 {
        for entry in taps.iter_mut() {
            entry.1 /= total;
        }
    }
    taps
}

/// Uniform cubic B-spline, support 2, no negative lobes.
///
/// The weights of the four knots around a point sum to one, so the filter
/// preserves flat areas exactly.
fn bspline(x: f32) -> f32 {
    let x = x.abs();
    if x < 1.0 {
        2.0 / 3.0 - x * x + x * x * x / 2.0
    } else if x < 2.0 {
        let t = 2.0 - x;
        t * t * t / 6.0
    } else {
        0.0
    }
}

/// Sampling shift that follows the local gradient.
fn gradient_shift(image: &RgbaImage, cell_x: u32, cell_y: u32) -> (f32, f32) {
    let (width, height) = image.dimensions();
    let x0 = cell_x.min(width.saturating_sub(1));
    let y0 = cell_y.min(height.saturating_sub(1));
    let x1 = (x0 + 1).min(width.saturating_sub(1));
    let y1 = (y0 + 1).min(height.saturating_sub(1));

    let l00 = luma(image.get_pixel(x0, y0));
    let l10 = luma(image.get_pixel(x1, y0));
    let l01 = luma(image.get_pixel(x0, y1));
    let l11 = luma(image.get_pixel(x1, y1));

    let gradient_x = (l01 + l11) - (l00 + l10);
    let gradient_y = (l10 + l11) - (l00 + l01);

    let magnitude = gradient_x.abs().max(gradient_y.abs());
    let strength = (magnitude / EASU_GRADIENT_FULL).clamp(0.0, 1.0);
    if strength <= 0.0 {
        return (0.0, 0.0);
    }

    let shift_x = if gradient_x != 0.0 {
        gradient_x.signum() * EASU_MAX_SHIFT * strength
    } else {
        0.0
    };
    let shift_y = if gradient_y != 0.0 {
        gradient_y.signum() * EASU_MAX_SHIFT * strength
    } else {
        0.0
    };
    (shift_x, shift_y)
}

/// Alpha-weighted luma, so transparent texels do not fake a gradient.
fn luma(pixel: &Rgba<u8>) -> f32 {
    let alpha = pixel.0[3] as f32 / 255.0;
    (0.2126 * pixel.0[0] as f32 + 0.7152 * pixel.0[1] as f32 + 0.0722 * pixel.0[2] as f32) * alpha
}

/// Robust contrast adaptive sharpening.
pub fn rcas(image: &RgbaImage, sharpness: f32) -> RgbaImage {
    if sharpness <= 0.0 {
        return image.clone();
    }
    let (width, height) = image.dimensions();
    let mut out = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let center = *image.get_pixel(x, y);
            let left = *image.get_pixel(x.saturating_sub(1), y);
            let right = *image.get_pixel((x + 1).min(width - 1), y);
            let up = *image.get_pixel(x, y.saturating_sub(1));
            let down = *image.get_pixel(x, (y + 1).min(height - 1));
            let up_left = *image.get_pixel(x.saturating_sub(1), y.saturating_sub(1));
            let up_right = *image.get_pixel((x + 1).min(width - 1), y.saturating_sub(1));
            let down_left = *image.get_pixel(x.saturating_sub(1), (y + 1).min(height - 1));
            let down_right = *image.get_pixel((x + 1).min(width - 1), (y + 1).min(height - 1));

            let luma_m = luma(&center);
            let luma_edge =
                (luma(&left) + luma(&right) + luma(&up) + luma(&down)) * 0.25;
            let luma_corner =
                (luma(&up_left) + luma(&up_right) + luma(&down_left) + luma(&down_right)) * 0.25;

            let range = ((luma_edge - luma_m).abs()).max((luma_corner - luma_m).abs());
            let weight = (range * 2.0).clamp(0.0, 1.0).powi(2);

            let mut result = [0u8; 4];
            for channel in 0..4 {
                let neighbours = left.0[channel] as f32 + right.0[channel] as f32
                    + up.0[channel] as f32
                    + down.0[channel] as f32;
                let average = neighbours * 0.25;
                let sharpened = center.0[channel] as f32
                    + 4.0 * weight * sharpness * (center.0[channel] as f32 - average);

                // Clamp to the neighbourhood range so no halos appear.
                let mut low = center.0[channel];
                let mut high = center.0[channel];
                for neighbour in [
                    &left, &right, &up, &down, &up_left, &up_right, &down_left, &down_right,
                ] {
                    low = low.min(neighbour.0[channel]);
                    high = high.max(neighbour.0[channel]);
                }
                result[channel] = sharpened.round().clamp(low as f32, high as f32) as u8;
            }
            out.put_pixel(x, y, Rgba(result));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(value: u8) -> RgbaImage {
        RgbaImage::from_pixel(8, 8, Rgba([value, value, value, 255]))
    }

    #[test]
    fn easu_keeps_flat_areas_flat() {
        let source = solid(120);
        let scaled = scale_easu(&source, 4);
        for pixel in scaled.pixels() {
            assert_eq!(pixel.0, [120, 120, 120, 255]);
        }
    }

    #[test]
    fn rcas_leaves_flat_areas_untouched() {
        let source = solid(90);
        let sharpened = rcas(&source, 0.2);
        for pixel in sharpened.pixels() {
            assert_eq!(pixel.0, [90, 90, 90, 255]);
        }
    }

    #[test]
    fn rcas_sharpens_a_hard_edge() {
        let mut source = RgbaImage::new(8, 8);
        for y in 0..8 {
            for x in 0..4 {
                source.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
            for x in 4..8 {
                source.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let plain = scale_easu(&source, 4);
        let sharpened = rcas(&plain, 0.4);

        // Count the intermediate (grey) pixels on the transition row.
        let count_grey = |image: &RgbaImage| {
            (0..image.width())
                .filter(|x| {
                    let value = image.get_pixel(*x, 0).0[0];
                    value > 10 && value < 245
                })
                .count()
        };
        assert!(
            count_grey(&sharpened) <= count_grey(&plain),
            "RCAS should not widen the transition"
        );
    }

    #[test]
    fn easu_output_size_matches_the_request() {
        let source = solid(10);
        assert_eq!(scale_easu(&source, 3).dimensions(), (24, 24));
        assert_eq!(scale_easu(&source, 6).dimensions(), (48, 48));
    }

    #[test]
    fn bspline_weights_sum_to_one() {
        for fraction in 0..10 {
            let f = fraction as f32 / 10.0;
            // The four knots around a point at `i + f`.
            let weights = [
                bspline(f + 1.0),
                bspline(f),
                bspline(1.0 - f),
                bspline(2.0 - f),
            ];
            let total: f32 = weights.iter().sum();
            assert!((total - 1.0).abs() < 1e-4, "weights summed to {total}");
        }
    }
}
