use std::path::PathBuf;

use clap::Parser;
use image::{RgbaImage, imageops};

use xbrstudio::model::{ModelFace, load_model, scale_model_faces_to_image};

const SHADE_FULL: u32 = 0xFF;
const SHADE_DIM: u32 = 0x88;
const BLEND_FILL: f32 = 0.55;

#[derive(Parser, Debug)]
#[command(
    name = "visualize",
    about = "Mark model-face UV rectangles on a texture, color-coded by face normal \
             (X=red, Y=green, Z=blue) with a distinct shade per face."
)]
struct Cli {
    /// Input PNG texture (e.g. assets/minecraft/textures/entity/cow/cow_temperate.png).
    texture: PathBuf,

    /// Model JSON with face UV rectangles.
    model: PathBuf,

    /// Output PNG path (defaults to <texture>_faces.png).
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Nearest-neighbor scale of the output image.
    #[arg(short, long, default_value_t = 1)]
    scale: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X = 0,
    Y = 1,
    Z = 2,
    Other = 3,
}

fn face_axis(face: &str) -> Axis {
    match face {
        "east" | "west" => Axis::X,
        "up" | "down" => Axis::Y,
        "north" | "south" => Axis::Z,
        _ => Axis::Other,
    }
}

/// One fill color per face: channel selects the normal axis
/// (X → ff0000-range, Y → 00ff00-range, Z → 0000ff-range), rank within the
/// axis fades that channel from 0xFF down to 0x88.
fn face_fill_colors(faces: &[ModelFace]) -> Vec<[u8; 3]> {
    let mut counts = [0usize; 4];
    for face in faces {
        counts[face_axis(&face.face) as usize] += 1;
    }

    let mut seen = [0usize; 4];
    faces
        .iter()
        .map(|face| {
            let axis = face_axis(&face.face) as usize;
            let rank = seen[axis];
            seen[axis] += 1;
            let total = counts[axis];
            let level = if total <= 1 {
                SHADE_FULL
            } else {
                SHADE_FULL - (rank as u32 * (SHADE_FULL - SHADE_DIM)) / (total as u32 - 1)
            } as u8;
            match axis {
                0 => [level, 0, 0],
                1 => [0, level, 0],
                2 => [0, 0, level],
                _ => [level, level, level],
            }
        })
        .collect()
}

fn overlay_faces(image: &RgbaImage, faces: &[ModelFace]) -> RgbaImage {
    let mut out = image.clone();
    let colors = face_fill_colors(faces);
    let (width, height) = out.dimensions();

    for (face, color) in faces.iter().zip(&colors) {
        let rect = face.rect;
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        let x_end = rect.right().min(width);
        let y_end = rect.bottom().min(height);
        for y in rect.y.min(height)..y_end {
            for x in rect.x.min(width)..x_end {
                let pixel = out.get_pixel_mut(x, y);
                for channel in 0..3 {
                    let base = pixel.0[channel] as f32;
                    let fill = color[channel] as f32;
                    pixel.0[channel] =
                        (base * (1.0 - BLEND_FILL) + fill * BLEND_FILL).round() as u8;
                }
            }
        }
    }
    out
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
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

    let mut marked = overlay_faces(&source, &model.faces);
    if cli.scale > 1 {
        marked = imageops::resize(
            &marked,
            marked.width() * cli.scale,
            marked.height() * cli.scale,
            imageops::FilterType::Nearest,
        );
    }

    let output = cli.output.clone().unwrap_or_else(|| {
        let stem = cli
            .texture
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        cli.texture.with_file_name(format!("{stem}_faces.png"))
    });
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
    }
    marked
        .save(&output)
        .map_err(|error| format!("cannot write {}: {error}", output.display()))?;

    let mut counts = [0usize; 4];
    for face in &model.faces {
        counts[face_axis(&face.face) as usize] += 1;
    }
    println!(
        "{}: {} faces (X {} / Y {} / Z {}) -> {} ({}x{})",
        cli.texture.display(),
        model.faces.len(),
        counts[0],
        counts[1],
        counts[2],
        output.display(),
        marked.width(),
        marked.height()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xbrstudio::model::FaceRect;

    fn face(name: &str) -> ModelFace {
        ModelFace {
            texture: String::new(),
            rect: FaceRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            face: name.into(),
            group: 0,
        }
    }

    #[test]
    fn colors_follow_face_normal_axis() {
        let colors = face_fill_colors(&[face("east"), face("west"), face("up"), face("north")]);
        assert_eq!(colors[0], [0xFF, 0, 0]);
        assert_eq!(colors[1], [0x88, 0, 0]);
        assert_eq!(colors[2], [0, 0xFF, 0]);
        assert_eq!(colors[3], [0, 0, 0xFF]);
    }

    #[test]
    fn shades_spread_from_full_to_dim_within_axis() {
        let colors = face_fill_colors(&[face("up"), face("down"), face("up")]);
        assert_eq!(colors[0][1], 0xFF);
        assert_eq!(colors[2][1], 0x88);
        assert!(colors[1][1] < colors[0][1] && colors[1][1] > colors[2][1]);
    }

    #[test]
    fn lone_face_stays_full_brightness() {
        let colors = face_fill_colors(&[face("south")]);
        assert_eq!(colors, vec![[0, 0, 0xFF]]);
    }

    #[test]
    fn unknown_face_falls_back_to_gray() {
        let colors = face_fill_colors(&[face("diagonal")]);
        assert_eq!(colors, vec![[0xFF, 0xFF, 0xFF]]);
    }
}
