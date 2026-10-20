#!/usr/bin/env python3
"""Register the ctm module and drop the unused enums."""
import sys

p = "src/lib.rs"
s = open(p).read()
if "pub mod ctm;" not in s:
    s = s.replace("pub mod config;", "pub mod config;\npub mod ctm;")
open(p, "w").write(s)

p = "src/ctm.rs"
s = open(p).read()
start = s.index("/// The four sides of a face, in OptiFine's order for the north face:")
end = s.index("/// OptiFine's tile selection.")
s = s[:start] + s[end:]
open(p, "w").write(s)
print("ctm registered")
