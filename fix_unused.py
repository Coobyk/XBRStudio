#!/usr/bin/env python3
"""Remove the unused imports of the new modules."""
import sys

p = "src/jar/entities.rs"
s = open(p).read()
s = s.replace("use std::path::{Path, PathBuf};", "use std::path::Path;")
s = s.replace(
    "use crate::model::{ModelFaces, parse_model, scale_model_faces_to_image};",
    "use crate::model::{ModelFaces, parse_model};",
)
open(p, "w").write(s)

p = "src/jar/preview.rs"
s = open(p).read()
s = s.replace("use std::path::{Path, PathBuf};", "use std::path::Path;")
s = s.replace("use crate::model::parse_model;\n\n", "")
s = s.replace(
    """use super::{
    TEXTURE_ROOT, animation_frame_size, collect_png_files, default_model_dir, entry_id,
    read_entry,
};""",
    """use super::{
    TEXTURE_ROOT, animation_frame_size, collect_png_files, entry_id, read_entry,
};""",
)
open(p, "w").write(s)
print("unused imports removed")
