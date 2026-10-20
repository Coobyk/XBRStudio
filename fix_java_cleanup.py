#!/usr/bin/env python3
"""Remove the unused helper in java.rs."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """fn numbers_in(text: &str) -> Vec<f32> {
    split_args(text)
        .iter()
        .filter_map(|arg| parse_number(arg))
        .collect()
}

"""
assert old in s
s = s.replace(old, "")
open(p, "w").write(s)
print("java.rs cleaned")
