#!/usr/bin/env python3
"""Add the connected textures flag to the CLI."""
import sys

p = "src/main.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:300])
        sys.exit(1)
    s = s.replace(old, new, 1)


swap(
    """    /// Only process textures whose path contains this substring.
    #[arg(long)]
    only: Option<String>,""",
    """    /// Also write OptiFine connected textures for blocks with several faces.
    #[arg(long)]
    connected_textures: bool,

    /// Only process textures whose path contains this substring.
    #[arg(long)]
    only: Option<String>,""",
)

swap(
    """        only: cli.only.clone(),
        resume: cli.resume || config.resume.unwrap_or(false),
        pack: pack_options,
    };""",
    """        only: cli.only.clone(),
        resume: cli.resume || config.resume.unwrap_or(false),
        pack: pack_options,
        connected_textures: cli.connected_textures
            || config.connected_textures.unwrap_or(false),
    };""",
)

swap(
    """    if report.stitched > 0 {
        println!(
            "  textures stitched with {} entity + {} block models: {}",
            report.entity_models, report.block_models, report.stitched
        );
    }""",
    """    if report.stitched > 0 {
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
    }""",
)

open(p, "w").write(s)
print("main.rs connected textures flag added")
