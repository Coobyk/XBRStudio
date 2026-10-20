#!/usr/bin/env python3
"""Import the entity helpers in the tests of jar.rs."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """#[cfg(test)]
mod tests {
    use super::*;"""
new = """#[cfg(test)]
mod tests {
    use super::entities::{
        best_model_for, entity_family_key, model_key, normalize_key, strip_key_prefix, texture_ctx,
    };
    use super::*;"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("test imports fixed")
