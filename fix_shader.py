#!/usr/bin/env python3
"""Rename the shadowed variable in the shader."""
import sys

p = "src/gpu_shader.wgsl"
s = open(p).read()

old = """    let dst = vec2<f32>(f32(gid.x), f32(gid.y));
    var center = (dst + vec2<f32>(0.5)) / scale - vec2<f32>(0.5);"""
new = """    let position = vec2<f32>(f32(gid.x), f32(gid.y));
    var center = (position + vec2<f32>(0.5)) / scale - vec2<f32>(0.5);"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("shader fixed")
