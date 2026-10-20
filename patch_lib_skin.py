#!/usr/bin/env python3
"""Register the skin module."""
import sys

p = "src/lib.rs"
s = open(p).read()
if "pub mod skin;" not in s:
    s = s.replace("pub mod scalers;", "pub mod scalers;\npub mod skin;")
open(p, "w").write(s)
print("skin registered")
