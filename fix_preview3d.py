#!/usr/bin/env python3
"""Fix the texture coordinate interpolation in the 3D renderer."""
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
    """        for triangle in [(0, 1, 2), (0, 2, 3)] {
            draw_triangle(
                &mut image,
                &mut depth,
                texture,
                &projected,
                triangle,
                quad.shade,
            );
        }""",
    """        for triangle in [(0, 1, 2), (0, 2, 3)] {
            draw_triangle(
                &mut image,
                &mut depth,
                texture,
                &quad.corners,
                &projected,
                triangle,
                quad.shade,
            );
        }""",
)

swap(
    """fn draw_triangle(
    image: &mut RgbaImage,
    depth: &mut [f32],
    texture: &RgbaImage,
    projected: &[([f32, f32], f32); 4],
    triangle: (usize, usize, usize),
    shade: f32,
) {""",
    """fn draw_triangle(
    image: &mut RgbaImage,
    depth: &mut [f32],
    texture: &RgbaImage,
    corners: &[Vertex; 4],
    projected: &[([f32, f32], f32); 4],
    triangle: (usize, usize, usize),
    shade: f32,
) {""",
)

swap(
    """            // The texture coordinates of the three corners.
            let uv0 = uv_of(texture, projected, triangle.0);
            let uv1 = uv_of(texture, projected, triangle.1);
            let uv2 = uv_of(texture, projected, triangle.2);
            let u = w0 * uv0[0] + w1 * uv1[0] + w2 * uv2[0];
            let v = w0 * uv0[1] + w1 * uv1[1] + w2 * uv2[1];""",
    """            // The texture coordinates of the three corners.
            let uv0 = corners[triangle.0].uv;
            let uv1 = corners[triangle.1].uv;
            let uv2 = corners[triangle.2].uv;
            let u = w0 * uv0[0] + w1 * uv1[0] + w2 * uv2[0];
            let v = w0 * uv0[1] + w1 * uv1[1] + w2 * uv2[1];""",
)

swap(
    """/// The texture coordinates of a corner.
fn uv_of(
    texture: &RgbaImage,
    projected: &[([f32, f32], f32); 4],
    index: usize,
) -> [f32; 2] {
    // The texture coordinates are stored in the vertex order of the quad, so
    // they are looked up by position instead of the projected index.
    let _ = texture;
    UVS.with(|uvs| uvs.borrow()[index])
}

thread_local! {
    static UVS: std::cell::RefCell<[[f32; 2]; 4]> = const { std::cell::RefCell::new([[0.0, 0.0]; 4]) };
}

""",
    "",
)

open(p, "w").write(s)
print("preview3d.rs fixed")
