use std::path::{Path, PathBuf};

use clap::Parser;

use xbrstudio::jar::{self, BatchOptions, BatchProgress};
use xbrstudio::model::{self, load_model};
use xbrstudio::upscaling::{self, upscale_image, UpscaleConfig};

#[derive(Parser, Debug)]
#[command(
    name = "xbrstudio",
    version,
    about = "Upscale Minecraft textures with xBRZ — one texture at a time, or every texture in a jar."
)]
struct Cli {
    /// Input PNG texture (omit when using --jar).
    input: Option<PathBuf>,

    /// Minecraft client jar: batch-upscale every texture into a resource-pack folder.
    #[arg(long)]
    jar: Option<PathBuf>,

    /// Output PNG path (single mode) or output directory (--jar mode).
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Optional Minecraft model JSON (block or entity) with face UV rectangles.
    #[arg(long)]
    model: Option<PathBuf>,

    /// Directory of entity model JSONs used for stitching in --jar mode.
    /// Defaults to models/entity when present.
    #[arg(long)]
    models: Option<PathBuf>,

    /// Integer xBRZ scale factor (2-6).
    #[arg(short, long, default_value_t = 4)]
    factor: u32,

    /// Upscale without model-face stitching.
    #[arg(long)]
    no_stitch: bool,

    /// Skip the 1px self-tiling border wrap for block textures.
    #[arg(long)]
    no_wrap: bool,
}

fn main() {
    let cli = Cli::parse();

    if let Err(error) = run(cli) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match (&cli.input, &cli.jar) {
        (Some(_), Some(_)) => Err("pass either a texture path or --jar, not both".into()),
        (None, None) => Err("provide a texture path, or --jar <file.jar>".into()),
        (Some(input), None) => run_single(&cli, input),
        (None, Some(jar)) => run_batch(&cli, jar),
    }
}

fn run_single(cli: &Cli, input: &Path) -> Result<(), String> {
    let source = image::open(input)
        .map_err(|error| format!("cannot read {}: {error}", input.display()))?
        .to_rgba8();

    let faces = match &cli.model {
        Some(path) => {
            let mut model_faces = load_model(path)?;
            model::scale_model_faces_to_image(&mut model_faces, &source);
            if model_faces.faces.is_empty() {
                return Err(format!(
                    "model {} has no face UV rectangles inside the texture",
                    path.display()
                ));
            }
            Some(model_faces)
        }
        None => None,
    };

    let config = UpscaleConfig {
        factor: cli.factor,
        stitch_faces: !cli.no_stitch && faces.is_some(),
    };

    let wrap =
        !cli.no_wrap && !config.stitch_faces && jar::is_block_texture_path(input);
    let output_image = if wrap {
        upscaling::upscale_wrapped(&source, cli.factor)?
    } else {
        upscale_image(
            &source,
            faces.as_ref().map(|faces| faces.faces.as_slice()),
            &config,
        )?
    };

    let output_path = cli
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(input, cli.factor));
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
    }

    output_image
        .save(&output_path)
        .map_err(|error| format!("cannot write {}: {error}", output_path.display()))?;

    println!(
        "{} -> {} ({}x{}, factor {}{})",
        input.display(),
        output_path.display(),
        output_image.width(),
        output_image.height(),
        cli.factor,
        if wrap { ", tiled wrap" } else { "" }
    );

    if let Some(faces) = faces {
        println!(
            "stitched {} model faces into {} groups",
            faces.faces.len(),
            upscaling::group_stitch_faces(&faces.faces, source.width(), source.height()).len()
        );
    }

    Ok(())
}

fn run_batch(cli: &Cli, jar_path: &Path) -> Result<(), String> {
    let out_dir = cli.output.clone().unwrap_or_else(|| {
        let stem = jar_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let parent = jar_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{stem}_x{}", cli.factor))
    });

    let opts = BatchOptions {
        factor: cli.factor,
        stitch: !cli.no_stitch,
        model_dir: cli.models.clone(),
        wrap: !cli.no_wrap,
    };

    let report = jar::upscale_jar(jar_path, &out_dir, &opts, |progress| match progress {
        BatchProgress::Started {
            total,
            entity_models,
        } => {
            println!("{total} textures; {entity_models} entity models loaded for stitching");
        }
        BatchProgress::Texture { done, total, name } => {
            if done == 1 || done == total || done % 25 == 0 {
                println!("[{done}/{total}] {name}");
            }
        }
    })?;

    println!(
        "{} -> {} ({} textures upscaled, factor {})",
        jar_path.display(),
        out_dir.display(),
        report.upscaled,
        cli.factor
    );
    if report.wrapped > 0 {
        println!("  block textures with tiled wrap: {}", report.wrapped);
    }
    if report.animated > 0 {
        println!("  frame-aware animated textures: {}", report.animated);
    }
    if report.stitched > 0 {
        println!(
            "  entity textures stitched with {} models: {}",
            report.entity_models, report.stitched
        );
    }
    if !report.errors.is_empty() {
        eprintln!("{} textures failed:", report.errors.len());
        for (path, message) in report.errors.iter().take(20) {
            eprintln!("  {path}: {message}");
        }
        if report.errors.len() > 20 {
            eprintln!("  … and {} more", report.errors.len() - 20);
        }
        if report.upscaled == 0 {
            return Err("every texture failed".into());
        }
    }

    Ok(())
}

fn default_output_path(input: &std::path::Path, factor: u32) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let extension = input
        .extension()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "png".to_string());

    input.with_file_name(format!("{stem}_x{factor}.{extension}"))
}
