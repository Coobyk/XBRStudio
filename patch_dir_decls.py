#!/usr/bin/env python3
"""Declare the item model and atlas entries in the directory reader."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """    let mut jobs = Vec::new();
    let mut block_model_entries = Vec::new();
    let mut stack = vec![dir.to_path_buf()];"""
new = """    let mut jobs = Vec::new();
    let mut block_model_entries = Vec::new();
    let mut item_model_entries = Vec::new();
    let mut atlas_entries = Vec::new();
    let mut stack = vec![dir.to_path_buf()];"""
if old in s:
    s = s.replace(old, new)
    open(p, "w").write(s)
    print("directory reader declares the entries")
else:
    print("MISSING")
    sys.exit(1)
