#!/usr/bin/env python3
"""Remove the GPU debug test."""
import sys

p = "src/gpu.rs"
s = open(p).read()

start = s.index("    #[test]\n    fn debug_difference() {")
end = s.index("    #[test]\n    fn the_gpu_matches_the_cpu() {")
s = s[:start] + s[end:]
open(p, "w").write(s)
print("debug test removed")
