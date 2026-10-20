//! Upscaling algorithms.
//!
//! `xbrz` is the pixel-art-correct default: it keeps hard edges and produces
//! the crisp, slightly blocky look xBRZ is known for. The other algorithms
//! exist for comparison and for smoother "premium" looks:
//!
//! * [`fsr`] — FSR-style edge-adaptive spatial upsampling (EASU) plus
//!   contrast-adaptive sharpening (RCAS).
//! * [`resample`] — separable windowed filters (Lanczos-3, Catmull-Rom).
//! * `nearest` — exact pixel doubling, for debugging and comparisons.

pub mod fsr;
pub mod resample;

use image::RgbaImage;

/// How transparent texels are treated while scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum AlphaMode {
    /// Interpolate every channel independently. Transparent texels still
    /// contribute their (usually black) colour, which shows up as dark
    /// fringes around leaves, glass and particles.
    #[default]
    Straight,
    /// Scale premultiplied colour, then unpremultiply. Transparent texels
    /// cannot bleed their colour into opaque neighbours.
    Premultiplied,
    /// Weight every tap by the alpha of the texel it reads, so fully
    /// transparent texels contribute nothing while the alpha channel itself
    /// is interpolated normally. Keeps hard alpha edges.
    Masked,
}

impl AlphaMode {
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "straight" | "none" => Ok(Self::Straight),
            "premultiplied" | "premul" => Ok(Self::Premultiplied),
            "masked" | "alpha" | "alpha-aware" => Ok(Self::Masked),
            other => Err(format!(
                "unknown alpha mode {other:?} (expected straight, premultiplied or masked)"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Straight => "straight",
            Self::Premultiplied => "premultiplied",
            Self::Masked => "masked",
        }
    }
}

/// The available upscaling algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Algorithm {
    /// xBRZ — the classic pixel-art upscaler.
    #[default]
    Xbrz,
    /// FSR-style edge-adaptive upsampling with contrast-adaptive sharpening.
    Fsr,
    /// Lanczos-3 windowed sinc.
    Lanczos3,
    /// Catmull-Rom cubic.
    Bicubic,
    /// Nearest neighbour.
    Nearest,
}

impl Algorithm {
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "xbrz" => Ok(Self::Xbrz),
            "fsr" => Ok(Self::Fsr),
            "lanczos3" | "lanczos" => Ok(Self::Lanczos3),
            "bicubic" | "catmull-rom" | "catmullrom" => Ok(Self::Bicubic),
            "nearest" | "pixel" => Ok(Self::Nearest),
            other => Err(format!(
                "unknown algorithm {other:?} (expected xbrz, fsr, lanczos3, bicubic or nearest)"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Xbrz => "xbrz",
            Self::Fsr => "fsr",
            Self::Lanczos3 => "lanczos3",
            Self::Bicubic => "bicubic",
            Self::Nearest => "nearest",
        }
    }

    /// Scale `image` by `factor` with this algorithm.
    ///
    /// Alpha is always handled with straight (non-premultiplied) channels;
    /// callers that want fringe-free output premultiply first.
    pub fn scale(self, image: &RgbaImage, factor: u32) -> Result<RgbaImage, String> {
        match self {
            Self::Xbrz => {
                let (width, height) = image.dimensions();
                let scaled = xbrz::scale_rgba(
                    image.as_raw(),
                    width as usize,
                    height as usize,
                    factor as usize,
                );
                RgbaImage::from_raw(width * factor, height * factor, scaled)
                    .ok_or_else(|| "xBRZ produced an invalid image buffer".to_string())
            }
            Self::Fsr => Ok(fsr::scale_fsr(image, factor)),
            Self::Lanczos3 => Ok(resample::scale_windowed(
                image,
                factor,
                &resample::Lanczos3,
                resample::AlphaHandling::Straight,
            )),
            Self::Bicubic => Ok(resample::scale_windowed(
                image,
                factor,
                &resample::CatmullRom,
                resample::AlphaHandling::Straight,
            )),
            Self::Nearest => Ok(resample::scale_nearest(image, factor)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn checkerboard(width: u32, height: u32) -> RgbaImage {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let value = if (x + y) % 2 == 0 { 255 } else { 0 };
            *pixel = Rgba([value, value, value, 255]);
        }
        image
    }

    #[test]
    fn names_round_trip() {
        for name in ["xbrz", "fsr", "lanczos3", "bicubic", "nearest"] {
            let algorithm = Algorithm::from_name(name).unwrap();
            assert_eq!(algorithm.name(), name);
        }
        assert!(Algorithm::from_name("nope").is_err());
    }

    #[test]
    fn alpha_mode_names_round_trip() {
        for name in ["straight", "premultiplied", "masked"] {
            let mode = AlphaMode::from_name(name).unwrap();
            assert_eq!(mode.name(), name);
        }
        assert!(AlphaMode::from_name("nope").is_err());
    }

    #[test]
    fn every_algorithm_scales_to_the_requested_size() {
        let source = checkerboard(16, 16);
        for algorithm in [
            Algorithm::Xbrz,
            Algorithm::Fsr,
            Algorithm::Lanczos3,
            Algorithm::Bicubic,
            Algorithm::Nearest,
        ] {
            let scaled = algorithm.scale(&source, 4).unwrap();
            assert_eq!(
                scaled.dimensions(),
                (64, 64),
                "{} produced the wrong size",
                algorithm.name()
            );
        }
    }

    #[test]
    fn constant_images_stay_constant() {
        let source = RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]));
        for algorithm in [
            Algorithm::Xbrz,
            Algorithm::Fsr,
            Algorithm::Lanczos3,
            Algorithm::Bicubic,
            Algorithm::Nearest,
        ] {
            let scaled = algorithm.scale(&source, 4).unwrap();
            // The border is skipped: xBRZ blends the outer pixels with the
            // transparent pixels outside the image, so the edge is softer.
            for y in 2..scaled.height() - 2 {
                for x in 2..scaled.width() - 2 {
                    assert_eq!(
                        scaled.get_pixel(x, y).0,
                        [10, 20, 30, 255],
                        "{} changed a constant image",
                        algorithm.name()
                    );
                }
            }
        }
    }
}
