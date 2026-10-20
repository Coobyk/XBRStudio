#!/usr/bin/env python3
"""Remove the dead assignment in build_axis_taps."""
p = "src/scalers/resample.rs"
s = open(p).read()
old = """            row.push((clamped, 1.0));
            total = 1.0;
"""
new = """            row.push((clamped, 1.0));
"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("ok")
