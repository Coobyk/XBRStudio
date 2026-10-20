#!/usr/bin/env python3
"""Fix the last clippy warnings."""
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


patch(
    "src/upscaling.rs",
    [
        (
            """                    for channel in 0..4 {
                        blurred[channel] += sample.0[channel] as f32 * weight;
                    }""",
            """                    for (channel, value) in blurred.iter_mut().enumerate() {
                        *value += sample.0[channel] as f32 * weight;
                    }""",
        ),
    ],
)

patch(
    "src/bin/gui.rs",
    [
        (
            """    if animation.frame_height == 0 {
        1
    } else {
        (height / animation.frame_height).max(1) as usize
    }""",
            """    height
        .checked_div(animation.frame_height)
        .map(|count| count.max(1) as usize)
        .unwrap_or(1)""",
        ),
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

patch(
    "src/bin/visualize.rs",
    [
        (
            """                for channel in 0..3 {""",
            """                for (channel, _) in color.iter().enumerate().take(3) {""",
        ),
    ],
)
