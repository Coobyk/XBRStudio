#!/usr/bin/env python3
"""Make the moved entity helpers visible to the other modules."""
import re
import sys

p = "src/jar/entities.rs"
s = open(p).read()

names = [
    "load_entity_models",
    "best_model_for",
    "texture_ctx",
    "signal_score",
    "score_model",
    "model_eligible",
    "model_key",
    "normalize_key",
    "entity_family_key",
    "strip_key_prefix",
    "strip_variant_tokens",
    "alias_targets",
    "equipment_target",
    "is_armor_method",
]

for name in names:
    s = re.sub(rf"^fn {name}\b", f"pub fn {name}", s, flags=re.M)

s = re.sub(r"^struct EntityModel\b", "pub struct EntityModel", s, flags=re.M)
s = re.sub(r"^struct TextureCtx\b", "pub struct TextureCtx", s, flags=re.M)

if "use super::collect_json_files;" not in s:
    s = s.replace(
        "use crate::model::{ModelFaces, parse_model, scale_model_faces_to_image};",
        "use crate::model::{ModelFaces, parse_model, scale_model_faces_to_image};\n"
        "use super::collect_json_files;",
    )

# The hash map is not used in this module.
if "HashMap" not in s.replace("use std::collections::HashMap;", ""):
    s = s.replace("use std::collections::HashMap;\n", "")

open(p, "w").write(s)
print("entities.rs exposes its helpers")
