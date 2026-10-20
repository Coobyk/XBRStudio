#!/usr/bin/env python3
"""Fix the clippy warnings in the gpu module."""
import sys

p = "src/gpu.rs"
s = open(p).read()

s = s.replace(
    "size: (out_width as u64 * out_height as u64 * 4) as u64,",
    "size: out_width as u64 * out_height as u64 * 4,",
)

# Move the test module to the end of the file.
start = s.index("#[cfg(test)]\nmod tests {")
end = s.index("/// Scale an image with the GPU, falling back to the CPU when there is no\n/// device.")
tests = s[start:end]
s = s[:start] + s[end:]
s = s.rstrip() + "\n\n" + tests.rstrip() + "\n"

open(p, "w").write(s)
print("gpu.rs cleaned")
