#!/usr/bin/env python3
"""Export the preview loader and drop the unused import."""
import sys

p = "src/jar.rs"
s = open(p).read()
old = "pub use preview::{PreviewData, PreviewPair};"
new = "pub use preview::{PreviewData, PreviewPair, load_preview_pairs};"
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)

p = "src/jar/preview.rs"
s = open(p).read()
s = s.replace("use std::io::Read;\n", "")
open(p, "w").write(s)
print("preview exported")
