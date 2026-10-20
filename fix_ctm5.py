#!/usr/bin/env python3
"""Move the connected texture write before the PNG encoding."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
    {
        if let Err(error) = write_connected_textures(out_dir, &stem, model, &output, factor) {
            return Err(error);
        }
    }

    if let Some(mcmeta) = out_mcmeta {"""

new = """    if let Some(mcmeta) = out_mcmeta {"""

assert old in s
s = s.replace(old, new)

old2 = """    let mut png = Vec::new();
    DynamicImage::ImageRgba8(output)"""

new2 = """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
    {
        write_connected_textures(out_dir, &stem, model, &output, opts.factor)?;
    }

    let mut png = Vec::new();
    DynamicImage::ImageRgba8(output)"""

assert old2 in s
s = s.replace(old2, new2)

open(p, "w").write(s)
print("ctm write moved")
