#!/usr/bin/env python3
"""Add connected textures to the batch options and the processing."""
import sys

p = "src/jar.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:300])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:300]))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """    /// What ends up in pack.mcmeta.
    pub pack: PackOptions,
}""",
    """    /// What ends up in pack.mcmeta.
    pub pack: PackOptions,
    /// Also write OptiFine connected textures for blocks with several
    /// different faces.
    pub connected_textures: bool,
}""",
)

swap(
    """            resume: false,
            pack: PackOptions::default(),
        }
    }
}""",
    """            resume: false,
            pack: PackOptions::default(),
            connected_textures: false,
        }
    }
}""",
)

swap(
    """use crate::cache::{Cache, fingerprint, settings_fingerprint};""",
    """use crate::cache::{Cache, fingerprint, settings_fingerprint};
use crate::ctm::{self, BlockFaces};""",
)

# Write the connected textures after the texture was written.
swap(
    """    if let Some(mcmeta) = out_mcmeta {""",
    """    if opts.connected_textures
        && let Some(model) = block_model
        && let Some(stem) = block_stem
    {
        if let Err(error) = write_connected_textures(out_dir, &stem, model, &output, factor) {
            return Err(error);
        }
    }

    if let Some(mcmeta) = out_mcmeta {""",
)

# The helper.
swap(
    """fn upscale_animated(""",
    """/// Write the OptiFine connected textures of a block.
///
/// The faces are cut out of the upscaled texture, so the tiles match the
/// pack the player sees.
fn write_connected_textures(
    out_dir: &Path,
    stem: &str,
    model: &ModelFaces,
    upscaled: &RgbaImage,
    factor: u32,
) -> Result<(), String> {
    let mut faces = BlockFaces::default();
    for name in ["up", "down", "north", "south", "east", "west"] {
        let Some(face) = model.faces.iter().find(|face| face.face == name) else {
            continue;
        };
        let x = face.rect.x * factor;
        let y = face.rect.y * factor;
        let width = face.rect.width * factor;
        let height = face.rect.height * factor;
        if x + width > upscaled.width() || y + height > upscaled.height() {
            continue;
        }
        let image = image::imageops::crop_imm(upscaled, x, y, width, height).to_image();
        faces.insert(name, image);
    }

    if faces.faces.len() < 4 {
        // Not enough faces for a connected texture.
        return Ok(());
    }

    let tiles = ctm::compose_tiles(&faces);
    ctm::write_block(out_dir, stem, stem, &tiles)
}

fn upscale_animated(""",
)

open(p, "w").write(s)
print("jar.rs connected textures wired")
