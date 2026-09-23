use std::path::PathBuf;

use clap::Parser;
use image::{Rgba, imageops};

use xbrstudio::model::{load_model, scale_model_faces_to_image};
use xbrstudio::upscaling::face_border_tile;

const OUTLINE: [u8; 3] = [255, 0, 255];
const OUTLINE_BLEND: f32 = 0.55;

#[derive(Parser, Debug)]
#[command(
    name = "slice",
    about = "Cut every model face out of a texture with a border of the \
             3D-adjacent face pixels. Writes one PNG per face (neighbor-bordered \
             interior + a translucent magenta outline on the face rect). \
             No upscaling, no stitching."
)]
struct Cli {
    /// Input PNG texture (e.g. assets/minecraft/textures/entity/cow/cow_temperate.png).
    texture: PathBuf,

    /// Model JSON with face UV rectangles.
    model: PathBuf,

    /// Output directory (one PNG per face).
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Border width in pixels, filled from the faces adjacent in 3D.
    #[arg(long, default_value_t = 4)]
    border: u32,

    /// Nearest-neighbor scale of each output image.
    #[arg(short, long, default_value_t = 4)]
    scale: u32,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn outline_pixel(pixel: Rgba<u8>) -> Rgba<u8> {
    let mix = |channel: u8, outline: u8| {
        (channel as f32 * (1.0 - OUTLINE_BLEND) + outline as f32 * OUTLINE_BLEND).round() as u8
    };
    Rgba([
        mix(pixel.0[0], OUTLINE[0]),
        mix(pixel.0[1], OUTLINE[1]),
        mix(pixel.0[2], OUTLINE[2]),
        pixel.0[3],
    ])
}

fn run(cli: Cli) -> Result<(), String> {
    if cli.scale == 0 {
        return Err("scale must be at least 1".into());
    }

    let source = image::open(&cli.texture)
        .map_err(|error| format!("cannot read {}: {error}", cli.texture.display()))?
        .to_rgba8();
    if source.width() == 0 || source.height() == 0 {
        return Err("zero-size texture".into());
    }

    let mut model = load_model(&cli.model)?;
    scale_model_faces_to_image(&mut model, &source);
    if model.faces.is_empty() {
        return Err(format!(
            "model {} has no face UV rectangles inside the texture",
            cli.model.display()
        ));
    }

    let output = cli.output.clone().unwrap_or_else(|| {
        let stem = cli.texture.file_stem().unwrap_or_default().to_string_lossy();
        cli.texture.with_file_name(format!("{stem}_slices"))
    });
    std::fs::create_dir_all(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;

    let border = cli.border;
    let mut written = 0usize;

    for (index, face) in model.faces.iter().enumerate() {
        let rect = face.rect;
        if rect.width == 0 || rect.height == 0 {
            continue;
        }

        let mut tile = face_border_tile(&source, &model.faces, index, border)?;

        let outline_left = border;
        let outline_top = border;
        let outline_right = outline_left + rect.width - 1;
        let outline_bottom = outline_top + rect.height - 1;
        for x in outline_left..=outline_right {
            let top = *tile.get_pixel(x, outline_top);
            tile.put_pixel(x, outline_top, outline_pixel(top));
            let bottom = *tile.get_pixel(x, outline_bottom);
            tile.put_pixel(x, outline_bottom, outline_pixel(bottom));
        }
        for y in outline_top..=outline_bottom {
            let left = *tile.get_pixel(outline_left, y);
            tile.put_pixel(outline_left, y, outline_pixel(left));
            let right = *tile.get_pixel(outline_right, y);
            tile.put_pixel(outline_right, y, outline_pixel(right));
        }

        let scaled = if cli.scale > 1 {
            imageops::resize(
                &tile,
                tile.width() * cli.scale,
                tile.height() * cli.scale,
                imageops::FilterType::Nearest,
            )
        } else {
            tile
        };

        let name = format!(
            "{index:02}_g{}_{}_{}x{}_{}x{}.png",
            face.group, face.face, rect.x, rect.y, rect.width, rect.height
        );
        let path = output.join(name);
        scaled
            .save(&path)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        written += 1;
        println!(
            "g{} {} -> {}x{} at ({}, {}) -> {} ({}x{})",
            face.group,
            face.face,
            rect.width,
            rect.height,
            rect.x,
            rect.y,
            path.display(),
            scaled.width(),
            scaled.height()
        );
    }

    println!(
        "{}: cut {written} faces (border {}px from neighbors, scale x{}) -> {}",
        cli.texture.display(),
        border,
        cli.scale,
        output.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xbrstudio::model::FaceRect;

    #[test]
    fn outline_blends_instead_of_replacing() {
        let base = Rgba([10, 20, 30, 255]);
        let out = outline_pixel(base);
        assert_ne!(out.0[0], base.0[0]);
        assert_ne!(out.0[1], base.0[1]);
        // Magenta-dominant blend: red and blue rise, green falls toward 0.
        assert!(out.0[0] > base.0[0]);
        assert!(out.0[2] > base.0[2]);
        assert!(out.0[1] < base.0[1]);
        // Still recognizably the base texture, not pure magenta.
        assert!(out.0[0] < 255 && out.0[1] > 0);
        assert_eq!(out.0[3], 255);
    }

    #[test]
    fn outline_keeps_alpha() {
        let out = outline_pixel(Rgba([0, 0, 0, 200]));
        assert_eq!(out.0[3], 200);
    }

    #[test]
    fn face_rect_fields_are_readable_for_naming() {
        let rect = FaceRect {
            x: 6,
            y: 6,
            width: 8,
            height: 8,
        };
        let name = format!("g0_north_{}x{}_{}x{}", rect.x, rect.y, rect.width, rect.height);
        assert_eq!(name, "g0_north_6x6_8x8");
    }
}
