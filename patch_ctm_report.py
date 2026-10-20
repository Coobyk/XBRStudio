#!/usr/bin/env python3
"""Count the connected textures in the report."""
import sys

p = "src/jar.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:300])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d)" % s.count(old))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """    /// Textures that were already up to date and were skipped.
    pub skipped: usize,
}""",
    """    /// Textures that were already up to date and were skipped.
    pub skipped: usize,
    /// Blocks that got OptiFine connected textures.
    pub connected_textures: usize,
}""",
)

swap(
    """struct ProcessOutcome {
    animated: bool,
    stitched: bool,
    wrapped: bool,
    copied: bool,
    /// The texture was already up to date and was not processed again.
    skipped: bool,
}""",
    """struct ProcessOutcome {
    animated: bool,
    stitched: bool,
    wrapped: bool,
    copied: bool,
    /// The texture was already up to date and was not processed again.
    skipped: bool,
    /// The block got connected textures.
    connected: bool,
}""",
)

swap(
    """                        Ok(ProcessOutcome {
                            animated: false,
                            stitched: false,
                            wrapped: false,
                            copied: false,
                            skipped: true,
                        })""",
    """                        Ok(ProcessOutcome {
                            animated: false,
                            stitched: false,
                            wrapped: false,
                            copied: false,
                            skipped: true,
                            connected: false,
                        })""",
)

swap(
    """        return Ok(ProcessOutcome {
            animated: false,
            stitched: false,
            wrapped: false,
            copied: true,
            skipped: false,
        });""",
    """        return Ok(ProcessOutcome {
            animated: false,
            stitched: false,
            wrapped: false,
            copied: true,
            skipped: false,
            connected: false,
        });""",
)

swap(
    """    Ok(ProcessOutcome {
        animated,
        stitched,
        wrapped: wrap,
        copied: false,
        skipped: false,
    })""",
    """    Ok(ProcessOutcome {
        animated,
        stitched,
        wrapped: wrap,
        copied: false,
        skipped: false,
        connected,
    })""",
)

swap(
    """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
    {
        write_connected_textures(out_dir, &stem, model, &output, opts.factor)?;
    }""",
    """    let mut connected = false;
    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
    {
        connected = write_connected_textures(out_dir, &stem, model, &output, opts.factor)?;
    }""",
)

swap(
    """fn write_connected_textures(
    out_dir: &Path,
    stem: &str,
    model: &ModelFaces,
    upscaled: &RgbaImage,
    factor: u32,
) -> Result<(), String> {""",
    """fn write_connected_textures(
    out_dir: &Path,
    stem: &str,
    model: &ModelFaces,
    upscaled: &RgbaImage,
    factor: u32,
) -> Result<bool, String> {""",
)

swap(
    """    if faces.faces.len() < 4 {
        // Not enough faces for a connected texture.
        return Ok(());
    }

    let tiles = ctm::compose_tiles(&faces);
    ctm::write_block(out_dir, stem, stem, &tiles)
}""",
    """    if faces.faces.len() < 4 {
        // Not enough faces for a connected texture.
        return Ok(false);
    }

    let tiles = ctm::compose_tiles(&faces);
    ctm::write_block(out_dir, stem, stem, &tiles)?;
    Ok(true)
}""",
)

swap(
    """        unmatched_entity: Vec::new(),
        skipped: 0,
    };""",
    """        unmatched_entity: Vec::new(),
        skipped: 0,
        connected_textures: 0,
    };""",
)

swap(
    """                if outcome.wrapped {
                    report.wrapped += 1;
                }""",
    """                if outcome.wrapped {
                    report.wrapped += 1;
                }
                if outcome.connected {
                    report.connected_textures += 1;
                }""",
)

open(p, "w").write(s)
print("jar.rs report updated")
