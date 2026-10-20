#!/usr/bin/env python3
"""Register the preview3d module."""
import sys

p = "src/lib.rs"
s = open(p).read()
if "pub mod preview3d;" not in s:
    s = s.replace("pub mod pack;", "pub mod pack;\npub mod preview3d;")
open(p, "w").write(s)
print("preview3d registered")
