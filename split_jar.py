#!/usr/bin/env python3
"""Split src/jar.rs into modules."""
import os
import re
import sys

source = open("src/jar.rs", encoding="utf-8").read()
lines = source.split("\n")


def find(pattern, start=0):
    for index in range(start, len(lines)):
        if lines[index].startswith(pattern):
            return index
    raise SystemExit("not found: " + pattern)


# The entity models and their scoring.
entities_start = find("struct EntityModel {")
entities_end = find("pub struct PreviewPair {")

# The preview data.
preview_start = entities_end
preview_end = find("fn entry_id(")

entities_block = "\n".join(lines[entities_start:entities_end]).rstrip() + "\n"
preview_block = "\n".join(lines[preview_start:preview_end]).rstrip() + "\n"

os.makedirs("src/jar", exist_ok=True)

header = """//! The entity models of the game and the way a texture is matched to one.

use super::*;

"""

with open("src/jar/entities.rs", "w", encoding="utf-8") as handle:
    handle.write(header + entities_block)

header = """//! The data of the 3D preview: the textures and the models that belong
//! together.

use super::*;

"""

with open("src/jar/preview.rs", "w", encoding="utf-8") as handle:
    handle.write(header + preview_block)

# The rest stays in jar.rs.
remaining = lines[:entities_start] + lines[preview_end:]

# Insert the module declarations after the imports.
text = "\n".join(remaining)
anchor = "pub const TEXTURE_ROOT: &str = \"assets/minecraft/textures/\";"
assert anchor in text
text = text.replace(
    anchor,
    "mod entities;\nmod preview;\n\npub use entities::EntityModel;\npub use preview::{PreviewData, PreviewPair};\n\n"
    + anchor,
)

# The moved items are used from the other module.
text = text.replace("\nstruct EntityModel {", "\npub struct EntityModel {")
for name in [
    "fn load_entity_models(",
    "fn load_block_models(",
    "fn collect_json_files(",
    "fn collect_png_files(",
    "fn block_texture_stem_for(",
    "fn entry_id(",
]:
    text = text.replace("\n" + name, "\npub " + name)

# The scoring helpers are used by the entity module.
for name in [
    "fn texture_ctx(",
    "fn signal_score(",
    "fn score_model(",
    "fn best_model_for(",
    "fn model_eligible(",
    "fn model_key(",
    "fn normalize_key(",
    "fn strip_key_prefix(",
    "fn strip_variant_tokens(",
    "fn alias_targets(",
    "fn equipment_target(",
    "fn is_armor_method(",
    "fn entity_family_key(",
]:
    text = text.replace("\n" + name, "\npub " + name)

with open("src/jar.rs", "w", encoding="utf-8") as handle:
    handle.write(text)

print("jar.rs split into modules")
