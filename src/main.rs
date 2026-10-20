use std::path::{Path, PathBuf};

use clap::Parser;

use xbrstudio::config::Config;
use xbrstudio::jar::{self, BatchOptions, BatchProgress};
use xbrstudio::model::{self, load_model};
use xbrstudio::pack::{self, PackOptions};
use xbrstudio::scalers::{Algorithm, AlphaMode};
use xbrstudio::upscaling::{self, UpscaleConfig, upscale_image};

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

    /// Upscaling algorithm.
    #[arg(long, value_enum, default_value_t = Algorithm::Xbrz)]
    algorithm: Algorithm,

    /// How transparent texels are handled while scaling.
    #[arg(long, value_enum, default_value_t = AlphaMode::Straight)]
    alpha: AlphaMode,

    /// Extra sharpening after scaling (0.0-1.0).
    #[arg(long, default_value_t = 0.0)]
    sharpen: f32,

    /// Upscale without model-face stitching.
    #[arg(long)]
    no_stitch: bool,

    /// Skip the 1px self-tiling border wrap for block textures and the beacon beam.
    #[arg(long)]
    no_wrap: bool,

    /// Also write the resource pack as a zip file.
    #[arg(long)]
    zip: Option<PathBuf>,

    /// Install the pack zip into the Minecraft resourcepacks folder.
    #[arg(long)]
    install: bool,

    /// `pack_format` written into pack.mcmeta.
    #[arg(long)]
    pack_format: Option<i64>,

    /// Description written into pack.mcmeta.
    #[arg(long)]
    pack_description: Option<String>,

    /// OptiFine mipmap levels written into pack.mcmeta.
    #[arg(long)]
    mipmap_levels: Option<u32>,

    /// Also write OptiFine connected textures for blocks with several faces.
    #[arg(long)]
    connected_textures: bool,

    /// Only process textures whose path contains this substring.
    #[arg(long)]
    only: Option<String>,

    /// Skip textures that are already up to date.
    #[arg(long)]
    resume: bool,

    /// Configuration file (default: ./xbrstudio.toml or the user config dir).
    #[arg(long)]
    config: Option<PathBuf>,

    /// Print more detail about what is going on.
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let cli = Cli::parse();

    if let Err(error) = run(cli) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let config = Config::load(cli.config.as_deref())?;

    let factor = if cli.factor == 4 {
        config.factor.unwrap_or(cli.factor)
    } else {
        cli.factor
    };
    let algorithm = config.algorithm.unwrap_or(cli.algorithm);
    let alpha = config.alpha.unwrap_or(cli.alpha);
    let sharpen = if cli.sharpen == 0.0 {
        config.sharpen.unwrap_or(cli.sharpen)
    } else {
        cli.sharpen
    };
    let stitch = config.stitch.unwrap_or(true) && !cli.no_stitch;
    let wrap = config.wrap.unwrap_or(true) && !cli.no_wrap;
    let model_dir = cli.models.clone().or_else(|| {
        config
            .models
            .as_ref()
            .map(|path| PathBuf::from(path))
            .or_else(jar::default_model_dir)
    });

    match (&cli.input, &cli.jar) {
        (Some(_), Some(_)) => Err("pass either a texture path or --jar, not both".into()),
        (None, None) => Err("provide a texture path, or --jar <file.jar>".into()),
        (Some(input), None) => run_single(
            &cli,
            input,
            factor,
            algorithm,
            alpha,
            sharpen,
            stitch,
            wrap,
        ),
        (None, Some(jar_path)) => run_batch(
            &cli,
            jar_path,
            factor,
            algorithm,
            alpha,
            sharpen,
            stitch,
            wrap,
            model_dir,
            &config,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_single(
    cli: &Cli,
    input: &Path,
    factor: u32,
    algorithm: Algorithm,
    alpha: AlphaMode,
    sharpen: f32,
    stitch: bool,
    wrap: bool,
) -> Result<(), String> {
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
        factor,
        stitch_faces: stitch && faces.is_some(),
        algorithm,
        alpha_mode: alpha,
        sharpen,
    };

    let use_wrap = wrap && !config.stitch_faces && jar::is_wrap_texture_path(input);
    let output_image = if use_wrap {
        upscaling::upscale_wrapped(&source, &config)?
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
        .unwrap_or_else(|| default_output_path(input, factor));
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }

    output_image
        .save(&output_path)
        .map_err(|error| format!("cannot write {}: {error}", output_path.display()))?;

    println!(
        "{} -> {} ({}x{}, factor {}, {})",
        input.display(),
        output_path.display(),
        output_image.width(),
        output_image.height(),
        factor,
        algorithm.name()
    );

    if config.stitch_faces
        && let Some(faces) = faces
    {
        println!(
            "stitched {} model faces across {} boxes",
            faces.faces.len(),
            upscaling::box_count(&faces.faces)
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_batch(
    cli: &Cli,
    jar_path: &Path,
    factor: u32,
    algorithm: Algorithm,
    alpha: AlphaMode,
    sharpen: f32,
    stitch: bool,
    wrap: bool,
    model_dir: Option<PathBuf>,
    config: &Config,
) -> Result<(), String> {
    let out_dir = cli.output.clone().unwrap_or_else(|| {
        let stem = jar_path.file_stem().unwrap_or_default().to_string_lossy();
        let parent = jar_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{stem}_x{factor}"))
    });

    let pack_options = PackOptions {
        pack_format: cli.pack_format.or(config.pack_format),
        description: cli.pack_description.clone().or(config.pack_description.clone()),
        mipmap_levels: cli.mipmap_levels.or(config.mipmap_levels),
        min_format: None,
        max_format: None,
    };

    let opts = BatchOptions {
        factor,
        stitch,
        model_dir,
        wrap,
        algorithm,
        alpha_mode: alpha,
        sharpen,
        only: cli.only.clone(),
        resume: cli.resume || config.resume.unwrap_or(false),
        pack: pack_options,
        connected_textures: cli.connected_textures
            || config.connected_textures.unwrap_or(false),
        cancel: None,
    };

    let report = jar::upscale_jar(jar_path, &out_dir, &opts, |progress| match progress {
        BatchProgress::Started {
            total,
            entity_models,
            block_models,
        } => {
            println!(
                "{total} textures; {entity_models} entity models, {block_models} block models for stitching"
            );
        }
        BatchProgress::Texture { done, total, name } => {
            if done == 1 || done == total || done % 25 == 0 {
                println!("[{done}/{total}] {name}");
            }
        }
    })?;

    println!(
        "{} -> {} ({} textures upscaled, {} colormaps copied, factor {}, {})",
        jar_path.display(),
        out_dir.display(),
        report.upscaled,
        report.copied,
        factor,
        algorithm.name()
    );
    if report.skipped > 0 {
        println!("  textures skipped (already up to date): {}", report.skipped);
    }
    if report.wrapped > 0 {
        println!("  textures with tiled wrap: {}", report.wrapped);
    }
    if report.animated > 0 {
        println!("  frame-aware animated textures: {}", report.animated);
    }
    if report.stitched > 0 {
        println!(
            "  textures stitched with {} entity + {} block models: {}",
            report.entity_models, report.block_models, report.stitched
        );
    }
    if report.connected_textures > 0 {
        println!(
            "  blocks with connected textures: {}",
            report.connected_textures
        );
    }
    if !report.unmatched_entity.is_empty() {
        println!(
            "  entity textures without a matching model: {}",
            report.unmatched_entity.len()
        );
        for path in report.unmatched_entity.iter().take(20) {
            println!("    {path}");
        }
        if report.unmatched_entity.len() > 20 {
            println!("    … and {} more", report.unmatched_entity.len() - 20);
        }
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

    if let Some(zip_path) = &cli.zip {
        pack::zip_directory(&out_dir, zip_path)?;
        println!("wrote {}", zip_path.display());
    }

    if cli.install {
        let zip_path = cli
            .zip
            .clone()
            .unwrap_or_else(|| out_dir.join(format!("{stem}.zip", stem = out_dir_name(&out_dir))));
        if !zip_path.is_file() {
            pack::zip_directory(&out_dir, &zip_path)?;
        }
        let installed = pack::install_pack(&zip_path)?;
        println!("installed {}", installed.display());
    }

    Ok(())
}

fn out_dir_name(out_dir: &Path) -> String {
    out_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "pack".to_string())
}

fn default_output_path(input: &std::path::Path, factor: u32) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let extension = input
        .extension()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "png".to_string());

    input.with_file_name(format!("{stem}_x{factor}.{extension}"))
}
