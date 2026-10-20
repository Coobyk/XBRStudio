#!/usr/bin/env python3
"""Make the moved entity helpers visible to the other modules."""
import sys

p = "src/jar/entities.rs"
s = open(p).read()

for name in [
    "fn load_entity_models(",
    "fn best_model_for(",
    "fn texture_ctx(",
    "fn signal_score(",
    "fn score_model(",
    "fn model_eligible(",
    "fn model_key(",
    "fn normalize_key(",
    "fn entity_family_key(",
    "fn strip_key_prefix(",
    "fn strip_variant_tokens(",
    "fn alias_targets(",
    "fn equipment_target(",
    "fn is_armor_method(",
]:
    s = s.replace("\n" + name, "\npub " + name)

s = s.replace(
    "use crate::model::{ModelFaces, parse_model, scale_model_faces_to_image};",
    "use crate::model::{ModelFaces, parse_model, scale_model_faces_to_image};\n"
    "use super::collect_json_files;",
)

open(p, "w").write(s)
print("entities.rs exposes its helpers")
