#!/usr/bin/env python3
"""Fix the remaining clippy warnings (after rustfmt)."""
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


patch(
    "src/scalers/fsr.rs",
    [
        (
            """            for channel in 0..4 {
                let neighbours = left.0[channel] as f32
                    + right.0[channel] as f32
                    + up.0[channel] as f32
                    + down.0[channel] as f32;""",
            """            for (channel, slot) in result.iter_mut().enumerate() {
                let neighbours = left.0[channel] as f32
                    + right.0[channel] as f32
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
            """            for channel in 0..4 {
                let average = blurred[channel] / weight_sum;
                let value =
                    center.0[channel] as f32 + amount * (center.0[channel] as f32 - average);
                result[channel] = value.round().clamp(0.0, 255.0) as u8;
            }""",
            """            for (channel, slot) in result.iter_mut().enumerate() {
                let average = blurred[channel] / weight_sum;
                let value =
                    center.0[channel] as f32 + amount * (center.0[channel] as f32 - average);
                *slot = value.round().clamp(0.0, 255.0) as u8;
            }""",
        ),
    ],
)

# The remaining small ones, found by their message.
with open("src/bin/visualize.rs", "r", encoding="utf-8") as handle:
    source = handle.read()
match = re.search(r"for \(index, pixel\) in (image|.*?)\.enumerate_pixels_mut\(\) \{\n(.*?)\n(.*?)\n", source, re.S)
print("visualize loop:", match.group(0)[:200] if match else "not found")
