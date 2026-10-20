use std::path::PathBuf;

use clap::Parser;

use xbrstudio::java;

#[derive(Parser, Debug)]
#[command(
    name = "extract-models",
    about = "Extract entity model JSONs from a decompiled Minecraft source tree. \
             Writes one JSON per model layer, laid out like models/entity."
)]
struct Cli {
    /// Directory with the decompiled Java sources.
    #[arg(long, default_value = ".")]
    source: PathBuf,

    /// Output directory for the model JSONs.
    #[arg(short, long, default_value = "models/entity")]
    output: PathBuf,

    /// Print every written file.
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    if !cli.source.is_dir() {
        return Err(format!("{} is not a directory", cli.source.display()));
    }

    let models = java::extract_tree(&cli.source);
    if models.is_empty() {
        return Err(format!(
            "no entity models found in {} (expected decompiled Java sources)",
            cli.source.display()
        ));
    }

    let written = java::write_models(&models, &cli.output)?;
    println!(
        "extracted {written} model layers from {} into {}",
        cli.source.display(),
        cli.output.display()
    );

    if cli.verbose {
        for (path, model) in &models {
            let cubes: usize = model
                .parts
                .iter()
                .map(|part| {
                    part.cubes.len()
                        + part
                            .children
                            .iter()
                            .map(|child| child.cubes.len())
                            .sum::<usize>()
                })
                .sum();
            println!(
                "  {} ({} parts, {cubes} cubes)",
                path.display(),
                model.parts.len()
            );
        }
    }

    Ok(())
}
