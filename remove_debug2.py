#!/usr/bin/env python3
"""Remove the debug test again."""
import sys

p = "src/jar.rs"
s = open(p).read()
start = s.index("    #[test]\n    fn debug_skipped_models() {")
end = s.index("    use super::*;")
s = s[:start] + s[end:]
open(p, "w").write(s)
print("debug test removed")
