#!/usr/bin/env python3
"""Pass the atlas sprites to the workers."""
import sys

p = "src/jar.rs"
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


# The signature of process_one.
swap(
    """fn process_one(
    name: &str,
    bytes: &[u8],
    mcmeta_bytes: Option<&[u8]>,
    out_dir: &Path,
    opts: &BatchOptions,
    models: &[EntityModel],
    block_models: &HashMap<String, ModelFaces>,
) -> Result<ProcessOutcome, String> {""",
    """#[allow(clippy::too_many_arguments)]
fn process_one(
    name: &str,
    bytes: &[u8],
    mcmeta_bytes: Option<&[u8]>,
    out_dir: &Path,
    opts: &BatchOptions,
    models: &[EntityModel],
    block_models: &HashMap<String, ModelFaces>,
    atlas_sprites: &std::collections::HashSet<String>,
) -> Result<ProcessOutcome, String> {""",
)

# The call sites.
swap(
    """                        let result = process_one(
                            &job.name,
                            &job.bytes,
                            job.mcmeta.as_deref(),
                            out_dir,
                            opts,
                            &models,
                            &block_models,
                        );""",
    """                        let result = process_one(
                            &job.name,
                            &job.bytes,
                            job.mcmeta.as_deref(),
                            out_dir,
                            opts,
                            &models,
                            &block_models,
                            &atlas_sprites,
                        );""",
)

swap(
    """                None => process_one(
                    &job.name,
                    &job.bytes,
                    job.mcmeta.as_deref(),
                    out_dir,
                    opts,
                    &models,
                    &block_models,
                ),""",
    """                None => process_one(
                    &job.name,
                    &job.bytes,
                    job.mcmeta.as_deref(),
                    out_dir,
                    opts,
                    &models,
                    &block_models,
                    &atlas_sprites,
                ),""",
)

# The connected textures are only written for the sprites of the atlas.
swap(
    """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
    {""",
    """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
        && (atlas_sprites.is_empty() || atlas_sprites.contains(&format!("block/{stem}.png")))
    {""",
)

open(p, "w").write(s)
print("atlas sprites reach the workers")
