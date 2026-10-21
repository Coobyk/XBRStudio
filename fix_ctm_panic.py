#!/usr/bin/env python3
"""Skip the empty faces of the connected textures."""
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
    "src/jar.rs",
    [
        (
            """        let image = image::imageops::crop_imm(upscaled, x, y, width, height).to_image();
        faces.insert(name, image);
    }

    if faces.faces.len() < 4 {
        // Not enough faces for a connected texture.
        return Ok(false);
    }""",
            """        if width == 0 || height == 0 {
            // A face without an area (a flat cube) has nothing to show.
            continue;
        }
        let image = image::imageops::crop_imm(upscaled, x, y, width, height).to_image();
        faces.insert(name, image);
    }

    if faces.faces.len() < 4 {
        // Not enough faces for a connected texture.
        return Ok(false);
    }""",
        ),
    ],
)

patch(
    "src/ctm.rs",
    [
        (
            """fn sample_scaled(image: &RgbaImage, x: u32, y: u32, size: u32) -> Rgba<u8> {
    if image.width() == 0 || image.height() == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let sx = (x as u64 * image.width() as u64 / size as u64) as u32;
    let sy = (y as u64 * image.height() as u64 / size as u64) as u32;
    *image.get_pixel(sx.min(image.width() - 1), sy.min(image.height() - 1))
}""",
            """fn sample_scaled(image: &RgbaImage, x: u32, y: u32, size: u32) -> Rgba<u8> {
    if image.width() == 0 || image.height() == 0 || size == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let sx = (x as u64 * image.width() as u64 / size as u64) as u32;
    let sy = (y as u64 * image.height() as u64 / size as u64) as u32;
    *image.get_pixel(
        sx.min(image.width() - 1),
        sy.min(image.height() - 1),
    )
}""",
        ),
    ],
)
