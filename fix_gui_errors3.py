#!/usr/bin/env python3
"""Import spawn_batch in the GUI and silence the unused variable."""
import sys

p = "src/bin/gui.rs"
s = open(p).read()

old = """use xbrstudio::jar::{self, BatchOptions, BatchProgress, JarMessage, is_wrap_texture_path};"""
new = """use xbrstudio::jar::{
    self, BatchOptions, BatchProgress, JarMessage, is_wrap_texture_path, spawn_batch,
};"""
assert old in s
s = s.replace(old, new)

old = """        let source_image = self.source_image.clone();"""
new = """        let _source_image = self.source_image.clone();"""
assert old in s
s = s.replace(old, new)

open(p, "w").write(s)
print("gui imports fixed")
