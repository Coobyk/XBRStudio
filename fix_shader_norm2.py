#!/usr/bin/env python3
"""Clean up the shader normalisation."""
import sys

p = "src/gpu_shader.wgsl"
s = open(p).read()

start = s.index("    // The weights are normalised per axis")
end = s.index("    let index = gid.y * params.dst_size.x + gid.x;")

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

    var colour = vec4<f32>(0.0);
    for (var j = 0; j < 4; j = j + 1) {
        if (weight_y[j] == 0.0) {
            continue;
        }
        let source_y = base.y + f32(j);
        for (var i = 0; i < 4; i = i + 1) {
            if (weight_x[i] == 0.0) {
                continue;
            }
            let source_x = base.x + f32(i);
            let weight = weight_x[i] * weight_y[j];
            let pixel = load(i32(source_x), i32(source_y));
            colour = colour + pixel * weight;
        }
    }

"""

s = s[:start] + new + s[end:]
open(p, "w").write(s)
print("shader cleaned")
