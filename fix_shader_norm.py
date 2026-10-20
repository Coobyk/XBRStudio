#!/usr/bin/env python3
"""Normalise the weights per axis, like the CPU implementation does."""
import sys

p = "src/gpu_shader.wgsl"
s = open(p).read()

old = """    let divisor = total_x * total_y;
    if (divisor > 1e-6) {
        colour = colour / divisor;
    }
"""
new = """    // The weights are normalised per axis, so the sum of the weights is one
    // in both directions.
    if (total_x > 1e-12) {
        for (var k = 0; k < 4; k = k + 1) {
            weight_x[k] = weight_x[k] / total_x;
        }
    }
    if (total_y > 1e-12) {
        for (var k = 0; k < 4; k = k + 1) {
            weight_y[k] = weight_y[k] / total_y;
        }
    }
    colour = colour / max(total_x * total_y, 1e-12);
"""
assert old in s
s = s.replace(old, new)

# Put the normalisation before the accumulation.
start = s.index("    var colour = vec4<f32>(0.0);")
end = s.index("    let index = gid.y * params.dst_size.x + gid.x;")
block = s[start:end]
normalise = """    // The weights are normalised per axis, so the sum of the weights is one
    // in both directions.
    if (total_x > 1e-12) {
        for (var k = 0; k < 4; k = k + 1) {
            weight_x[k] = weight_x[k] / total_x;
        }
    }
    if (total_y > 1e-12) {
        for (var k = 0; k < 4; k = k + 1) {
            weight_y[k] = weight_y[k] / total_y;
        }
    }

"""
s = s[:start] + normalise + block + s[end:]

# The diagnostic is replaced by the real output.
old_out = """    dst[index] = pack4x8unorm(vec4<f32>(divisor * 100000.0, total_x, total_y, 1.0));"""
new_out = """    dst[index] = pack4x8unorm(clamp(colour, vec4<f32>(0.0), vec4<f32>(1.0)));"""
assert old_out in s
s = s.replace(old_out, new_out)

open(p, "w").write(s)
print("shader normalised")
