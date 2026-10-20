#!/usr/bin/env python3
"""Add the skin converter to the CLI."""
import sys

p = "src/main.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:400])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:400]))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """    /// Optional Minecraft model JSON (block or entity) with face UV rectangles.
    #[arg(long)]
    model: Option<PathBuf>,""",
    """    /// Optional Minecraft model JSON (block or entity) with face UV rectangles.
    #[arg(long)]
    model: Option<PathBuf>,

    /// Convert a legacy 64x32 skin into the layout of the model.
    #[arg(long, value_name = "PNG")]
    convert_skin: Option<PathBuf>,""",
)

swap(
    """    match (&cli.input, &cli.jar) {
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
}""",
    """    if let Some(legacy) = &cli.convert_skin {
        return run_convert_skin(&cli, legacy);
    }

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

/// Convert a legacy skin into the layout of an entity model.
fn run_convert_skin(cli: &Cli, legacy_path: &Path) -> Result<(), String> {
    let Some(model_path) = &cli.model else {
        return Err("--convert-skin needs a model: pass --model <model.json>".into());
    };

    let legacy = image::open(legacy_path)
        .map_err(|error| format!("cannot read {}: {error}", legacy_path.display()))?
        .to_rgba8();
    let model = load_model(model_path)?;

    let modern = xbrstudio::skin::convert(&legacy, &model)?;

    let output_path = cli.output.clone().unwrap_or_else(|| {
        let stem = legacy_path.file_stem().unwrap_or_default().to_string_lossy();
        legacy_path.with_file_name(format!("{stem}_modern.png"))
    });
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    modern
        .save(&output_path)
        .map_err(|error| format!("cannot write {}: {error}", output_path.display()))?;

    println!(
        "{} -> {} ({}x{})",
        legacy_path.display(),
        output_path.display(),
        modern.width(),
        modern.height()
    );
    Ok(())
}""",
)

open(p, "w").write(s)
print("main.rs skin converter added")
