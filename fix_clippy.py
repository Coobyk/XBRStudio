#!/usr/bin/env python3
"""Fix the remaining clippy warnings."""
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


# The loop in jar.rs that strips a variant token.
patch(
    "src/jar.rs",
    [
        (
            """    loop {
        let Some(token) = ["baby", "pup", "cold", "warm", "temperate"]
            .into_iter()
            .find(|token| text.len() > token.len() && text.ends_with(token))
        else {
            break;
        };""",
            """    while let Some(token) = ["baby", "pup", "cold", "warm", "temperate"]
        .into_iter()
        .find(|token| text.len() > token.len() && text.ends_with(token))
    {""",
        ),
        (
            """        } else if key.starts_with(ctx.stripped.as_str()) && key.len() > ctx.stripped.len() {
            signal += 3000;
        } else if ctx.stripped.starts_with(key) && ctx.stripped.len() > key.len() {
            signal += 3000;
        }""",
            """        } else if (key.starts_with(ctx.stripped.as_str()) && key.len() > ctx.stripped.len())
            || (ctx.stripped.starts_with(key) && ctx.stripped.len() > key.len())
        {
            signal += 3000;
        }""",
        ),
    ],
)

# The channel loops.
patch(
    "src/scalers/fsr.rs",
    [
        (
            """            let mut result = [0u8; 4];
            for channel in 0..4 {
                let neighbours = left.0[channel] as f32 + right.0[channel] as f32
                    + up.0[channel] as f32
                    + down.0[channel] as f32;""",
            """            let mut result = [0u8; 4];
            for (channel, slot) in result.iter_mut().enumerate() {
                let neighbours = left.0[channel] as f32 + right.0[channel] as f32
                    + up.0[channel] as f32
                    + down.0[channel] as f32;""",
        ),
        (
            """                result[channel] = sharpened.round().clamp(low as f32, high as f32) as u8;""",
            """                *slot = sharpened.round().clamp(low as f32, high as f32) as u8;""",
        ),
    ],
)

patch(
    "src/upscaling.rs",
    [
        (
            """            let mut result = [0u8; 4];
            for channel in 0..4 {
                let average = blurred[channel] / weight_sum;
                let value = center.0[channel] as f32 + amount * (center.0[channel] as f32 - average);
                result[channel] = value.round().clamp(0.0, 255.0) as u8;
            }""",
            """            let mut result = [0u8; 4];
            for (channel, slot) in result.iter_mut().enumerate() {
                let average = blurred[channel] / weight_sum;
                let value = center.0[channel] as f32 + amount * (center.0[channel] as f32 - average);
                *slot = value.round().clamp(0.0, 255.0) as u8;
            }""",
        ),
    ],
)

patch(
    "src/bin/visualize.rs",
    [
        (
            """    for (index, pixel) in image.enumerate_pixels_mut() {""",
            """    for (index, pixel) in image.enumerate_pixels_mut() {""",
        ),
    ],
)

patch(
    "src/bin/gui.rs",
    [
        (
            """    for pixel in (&mut *bgra).chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }""",
            """    for pixel in bgra.pixels_mut() {
        pixel.0.swap(0, 2);
    }""",
        ),
    ],
)
