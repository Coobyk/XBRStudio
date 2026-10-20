#!/usr/bin/env python3
"""Show the weights the shader computes."""
import sys

p = "src/gpu_shader.wgsl"
s = open(p).read()
old = """    dst[index] = pack4x8unorm(clamp(colour, vec4<f32>(0.0), vec4<f32>(1.0)));"""
new = """    dst[index] = pack4x8unorm(vec4<f32>(divisor * 100000.0, total_x, total_y, 1.0));"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("shader diagnostic added")
