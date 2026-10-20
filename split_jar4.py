#!/usr/bin/env python3
"""Split src/jar.rs into jar/entities.rs and jar/preview.rs (final)."""
import os
import sys

path = "src/jar.rs"
lines = open(path, encoding="utf-8").read().split("\n")


def block(start_line, end_line):
    return "\n".join(lines[start_line - 1 : end_line - 1]).rstrip() + "\n"


entity_items = [
    (156, 173),
    (854, 861),
    (861, 867),
    (867, 876),
    (876, 890),
    (890, 899),
    (899, 919),
    (919, 952),
    (952, 967),
    (967, 989),
    (989, 994),
    (994, 998),
    (998, 1136),
    (1136, 1154),
    (1154, 1186),
    (1186, 1214),
    (1214, 1276),
]

preview_items = [
    (1276, 1295),
    (1295, 1304),
    (1304, 1404),
]

entities = "".join(block(start, end) for start, end in entity_items)
preview = "".join(block(start, end) for start, end in preview_items)

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
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use zip::ZipArchive;

use crate::model::parse_model;

use super::entities::{best_model_for, load_entity_models, texture_ctx};
use super::{
    TEXTURE_ROOT, animation_frame_size, collect_png_files, default_model_dir, entry_id,
    read_entry,
};

"""
        + preview
    )

text = "\n".join(remaining)

anchor = 'pub const TEXTURE_ROOT: &str = "assets/minecraft/textures/";'
assert anchor in text
text = text.replace(
    anchor,
    "mod entities;\nmod preview;\n\n"
    "use entities::{best_model_for, load_entity_models, texture_ctx};\n"
    "pub use entities::EntityModel;\n"
    "pub use preview::{PreviewData, PreviewPair};\n\n" + anchor,
)

for name in [
    "fn read_entry(",
    "fn collect_png_files(",
    "fn collect_json_files(",
    "fn entry_id(",
]:
    text = text.replace("\n" + name, "\npub(crate) " + name)

open(path, "w", encoding="utf-8").write(text)
print("jar.rs split into jar/entities.rs and jar/preview.rs")
