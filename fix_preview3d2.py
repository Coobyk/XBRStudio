#!/usr/bin/env python3
"""Use plain tuples for the projected points."""
import sys

p = "src/preview3d.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:300])
        sys.exit(1)
    s = s.replace(old, new, 1)


swap(
    """    let project = |position: [f32; 3]| -> ([f32, f32], f32) {""",
    """    let project = |position: [f32; 3]| -> (f32, f32, f32) {""",
)

swap(
    """        let projected: Vec<([f32, f32], f32)> = quad""",
    """        let projected: Vec<(f32, f32, f32)> = quad""",
)

swap(
    """    projected: &[([f32, f32], f32); 4],""",
    """    projected: &[(f32, f32, f32); 4],""",
)

# The tuple accessors change: .0 .0 -> .0, .0 .1 -> .1, .1 -> .2
swap(
    """    let (a, b, c) = (
        projected[triangle.0],
        projected[triangle.1],
        projected[triangle.2],
    );
    let area = (b.0 .0 - a.0 .0) * (c.0 .1 - a.0 .1) - (b.0 .1 - a.0 .1) * (c.0 .0 - a.0 .0);""",
    """    let (a, b, c) = (
        projected[triangle.0],
        projected[triangle.1],
        projected[triangle.2],
    );
    let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);""",
)

swap(
    """    let min_x = a.0 .0.min(b.0 .0).min(c.0 .0).floor().max(0.0) as u32;
    let max_x = (a.0 .0.max(b.0 .0).max(c.0 .0).ceil() as i64).clamp(0, image.width() as i64 - 1)
        as u32;
    let min_y = a.0 .1.min(b.0 .1).min(c.0 .1).floor().max(0.0) as u32;
    let max_y = (a.0 .1.max(b.0 .1).max(c.0 .1).ceil() as i64).clamp(0, image.height() as i64 - 1)
        as u32;""",
    """    let min_x = a.0.min(b.0).min(c.0).floor().max(0.0) as u32;
    let max_x = (a.0.max(b.0).max(c.0).ceil() as i64).clamp(0, image.width() as i64 - 1) as u32;
    let min_y = a.1.min(b.1).min(c.1).floor().max(0.0) as u32;
    let max_y = (a.1.max(b.1).max(c.1).ceil() as i64).clamp(0, image.height() as i64 - 1) as u32;""",
)

swap(
    """            let w0 = ((b.0 .0 - a.0 .0) * (py - a.0 .1) - (b.0 .1 - a.0 .1) * (px - a.0 .0)) / area;
            let w1 = ((px - a.0 .0) * (c.0 .1 - a.0 .1) - (py - a.0 .1) * (c.0 .0 - a.0 .0)) / area;""",
    """            let w0 = ((b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0)) / area;
            let w1 = ((px - a.0) * (c.1 - a.1) - (py - a.1) * (c.0 - a.0)) / area;""",
)

swap(
    """            let z = w0 * a.1 + w1 * b.1 + w2 * c.1;""",
    """            let z = w0 * a.2 + w1 * b.2 + w2 * c.2;""",
)

open(p, "w").write(s)
print("preview3d tuples fixed")
