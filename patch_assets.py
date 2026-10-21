#!/usr/bin/env python3
"""Register the assets module, add the UV fit score and read the item models."""
import re
import sys


def patch(path, pairs):
    with open(path, "r", encoding="utf-8") as handle:
        source = handle.read()
    for old, new in pairs:
        if old not in source:
            print("MISSING in %s:\n%s" % (path, old[:300]))
            sys.exit(1)
        if source.count(old) != 1:
            print("NOT UNIQUE in %s (%d):\n%s" % (path, source.count(old), old[:300]))
            sys.exit(1)
        source = source.replace(old, new)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(source)
    print("patched", path)


# The module.
patch(
    "src/lib.rs",
    [
        (
            """pub mod cache;""",
            """pub mod assets;
pub mod cache;""",
        ),
    ],
)

# The UV fit score.
patch(
    "src/jar/entities.rs",
    [
        (
            """pub fn score_model(model: &EntityModel, ctx: &TextureCtx, image: (u32, u32)) -> i64 {""",
            """/// How well the UV space of a model fits the texture.
///
/// A model whose UV space is bigger than the texture is squeezed onto it,
/// which makes the stitching blurry, so such a model is penalised. A model
/// that is smaller than the texture is stretched, which the game does as
/// well, so it is not.
pub fn uv_fit_score(model: &EntityModel, image: (u32, u32)) -> i64 {
    if image.0 == 0 || image.1 == 0 {
        return 0;
    }
    let (uv_width, uv_height) = model.faces.uv_size;
    if uv_width <= 0.0 || uv_height <= 0.0 {
        return 0;
    }
    let over_x = (uv_width - image.0 as f32).max(0.0) / uv_width;
    let over_y = (uv_height - image.1 as f32).max(0.0) / uv_height;
    let over = over_x.max(over_y);
    if over <= 0.0 {
        return 0;
    }
    -((over * 4000.0) as i64)
}

pub fn score_model(model: &EntityModel, ctx: &TextureCtx, image: (u32, u32)) -> i64 {""",
        ),
        (
            """    let mut score = signal_score(model, ctx)
        + model.faces.faces.len() as i64
        + if model.has_texture_size { 300 } else { 0 };""",
            """    let mut score = signal_score(model, ctx)
        + model.faces.faces.len() as i64
        + if model.has_texture_size { 300 } else { 0 }
        + uv_fit_score(model, image);""",
        ),
    ],
)
