#!/usr/bin/env python3
"""Pass the projected points as a slice."""
import sys

p = "src/preview3d.rs"
s = open(p).read()

old = """    projected: &[(f32, f32, f32); 4],"""
new = """    projected: &[(f32, f32, f32)],"""
assert old in s
s = s.replace(old, new)

old = """    let (a, b, c) = (
        projected[triangle.0],
        projected[triangle.1],
        projected[triangle.2],
    );"""
new = """    let (a, b, c) = (
        projected[triangle.0],
        projected[triangle.1],
        projected[triangle.2],
    );
"""
assert old in s
s = s.replace(old, new)

open(p, "w").write(s)
print("ok")
