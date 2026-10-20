#!/usr/bin/env python3
"""Split src/jar.rs into jar/entities.rs and jar/preview.rs."""
import os
import sys

path = "src/jar.rs"
lines = open(path, encoding="utf-8").read().split("\n")


def block(start_line, end_line):
    """The lines of an item, without the trailing blank line."""
    return "\n".join(lines[start_line - 1 : end_line - 1]).rstrip() + "\n"


# The entity models and the way a texture is matched to one.
entity_items = [
    (156, 173),  # struct EntityModel
    (854, 861),  # normalize_key
    (861, 867),  # model_key
    (867, 876),  # entity_family_key
    (876, 890),  # struct TextureCtx
    (890, 899),  # strip_key_prefix
    (899, 919),  # strip_variant_tokens
    (919, 952),  # texture_ctx
    (952, 967),  # alias_targets
    (967, 989),  # equipment_target
    (989, 994),  # const MIN_SIGNAL
    (994, 998),  # is_armor_method
    (998, 1136),  # signal_score
    (1136, 1154),  # score_model
    (1154, 1186),  # best_model_for
    (1186, 1214),  # model_eligible
    (1214, 1278),  # load_entity_models
]

preview_items = [
    (1278, 1295),  # PreviewPair
    (1295, 1304),  # PreviewData
    (1304, 1404),  # load_preview_pairs
]

entities = "".join(block(start, end) for start, end in entity_items)
preview = "".join(block(start, end) for start, end in preview_items)

# Everything that moves out of jar.rs.
moved = set()
for start, end in entity_items + preview_items:
    for index in range(start - 1, end - 1):
        moved.add(index)

remaining = [line for index, line in enumerate(lines) if index not in moved]

os.makedirs("src/jar", exist_ok=True)

with open("src/jar/entities.rs", "w", encoding="utf-8") as handle:
    handle.write(
        """//! The entity models of the game and the way a texture is matched to one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::{ModelFaces, parse_model, scale_model_faces_to_image};

"""
        + entities
    )

with open("src/jar/preview.rs", "w", encoding="utf-8") as handle:
    handle.write(
        """//! The data of the 3D preview: the textures and the models that belong
//! together.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use zip::ZipArchive;

use crate::model::parse_model;

use super::entities::{load_entity_models, texture_ctx};
use super::{collect_png_files, default_model_dir, read_entry};

"""
        + preview
    )

text = "\n".join(remaining)

# The module declarations.
anchor = 'pub const TEXTURE_ROOT: &str = "assets/minecraft/textures/";'
assert anchor in text
text = text.replace(
    anchor,
    "mod entities;\nmod preview;\n\n"
    "pub use entities::EntityModel;\n"
    "pub use preview::{PreviewData, PreviewPair};\n\n" + anchor,
)

# The items the submodules use.
for name in [
    "fn read_entry(",
    "fn collect_png_files(",
    "fn collect_json_files(",
]:
    text = text.replace("\n" + name, "\npub(crate) " + name)

open(path, "w", encoding="utf-8").write(text)
print("jar.rs split into jar/entities.rs and jar/preview.rs")
