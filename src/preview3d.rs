//! A small software 3D renderer for model previews.
//!
//! The renderer turns the faces of a model into textured quads and draws
//! them with a z-buffer. It is deliberately simple: it exists so the app can
//! show a model without a browser or a GPU pipeline.

use image::{Rgba, RgbaImage};

use crate::model::{ModelFace, ModelFaces};

/// One corner of a quad: a position in model space and a texture coordinate.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// A quad with its four corners.
#[derive(Debug, Clone)]
pub struct Quad {
    pub corners: [Vertex; 4],
    /// The face colour, used to shade the face by its normal.
    pub shade: f32,
}

/// A model ready to be drawn.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub quads: Vec<Quad>,
}

/// Build the mesh of a model.
///
/// The faces must carry the cube they belong to (entity models do, block
/// models do not).
pub fn build_mesh(model: &ModelFaces) -> Mesh {
    let mut quads = Vec::new();
    for face in &model.faces {
        let Some(quad) = quad_of(face) else {
            continue;
        };
        quads.push(quad);
    }
    Mesh { quads }
}

/// The quad of a single face.
fn quad_of(face: &ModelFace) -> Option<Quad> {
    let origin = face.origin?;
    let size = face.size?;
    let [ox, oy, oz] = origin;
    let [sx, sy, sz] = size;

    let rect = face.rect;
    let u0 = rect.x as f32;
    let v0 = rect.y as f32;
    let u1 = rect.right() as f32;
    let v1 = rect.bottom() as f32;

    let corners: [Vertex; 4] = match face.face.as_str() {
        "up" => [
            Vertex {
                position: [ox, oy + sy, oz],
                uv: [u0, v0],
            },
            Vertex {
                position: [ox, oy + sy, oz + sz],
                uv: [u0, v1],
            },
            Vertex {
                position: [ox + sx, oy + sy, oz + sz],
                uv: [u1, v1],
            },
            Vertex {
                position: [ox + sx, oy + sy, oz],
                uv: [u1, v0],
            },
        ],
        "down" => [
            Vertex {
                position: [ox, oy, oz],
                uv: [u0, v0],
            },
            Vertex {
                position: [ox + sx, oy, oz],
                uv: [u1, v0],
            },
            Vertex {
                position: [ox + sx, oy, oz + sz],
                uv: [u1, v1],
            },
            Vertex {
                position: [ox, oy, oz + sz],
                uv: [u0, v1],
            },
        ],
        "north" => [
            Vertex {
                position: [ox, oy, oz],
                uv: [u0, v1],
            },
            Vertex {
                position: [ox, oy + sy, oz],
                uv: [u0, v0],
            },
            Vertex {
                position: [ox + sx, oy + sy, oz],
                uv: [u1, v0],
            },
            Vertex {
                position: [ox + sx, oy, oz],
                uv: [u1, v1],
            },
        ],
        "south" => [
            Vertex {
                position: [ox + sx, oy, oz + sz],
                uv: [u0, v1],
            },
            Vertex {
                position: [ox + sx, oy + sy, oz + sz],
                uv: [u0, v0],
            },
            Vertex {
                position: [ox, oy + sy, oz + sz],
                uv: [u1, v0],
            },
            Vertex {
                position: [ox, oy, oz + sz],
                uv: [u1, v1],
            },
        ],
        "east" => [
            Vertex {
                position: [ox + sx, oy, oz + sz],
                uv: [u0, v1],
            },
            Vertex {
                position: [ox + sx, oy + sy, oz + sz],
                uv: [u0, v0],
            },
            Vertex {
                position: [ox + sx, oy + sy, oz],
                uv: [u1, v0],
            },
            Vertex {
                position: [ox + sx, oy, oz],
                uv: [u1, v1],
            },
        ],
        "west" => [
            Vertex {
                position: [ox, oy, oz],
                uv: [u0, v1],
            },
            Vertex {
                position: [ox, oy + sy, oz],
                uv: [u0, v0],
            },
            Vertex {
                position: [ox, oy + sy, oz + sz],
                uv: [u1, v0],
            },
            Vertex {
                position: [ox, oy, oz + sz],
                uv: [u1, v1],
            },
        ],
        _ => return None,
    };

    // A simple directional shade so the shape is readable.
    let shade = match face.face.as_str() {
        "up" => 1.0,
        "down" => 0.55,
        "north" | "south" => 0.8,
        _ => 0.68,
    };

    Some(Quad { corners, shade })
}

/// Render a mesh with an orthographic projection.
pub fn render(mesh: &Mesh, texture: &RgbaImage, width: u32, height: u32, yaw: f32, pitch: f32) -> RgbaImage {
    let mut image = RgbaImage::new(width, height);
    let mut depth = vec![f32::INFINITY; (width as usize) * (height as usize)];

    if mesh.quads.is_empty() {
        return image;
    }

    // The model is centred on its own bounds and scaled to fit.
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for quad in &mesh.quads {
        for corner in &quad.corners {
            for axis in 0..3 {
                min[axis] = min[axis].min(corner.position[axis]);
                max[axis] = max[axis].max(corner.position[axis]);
            }
        }
    }
    let centre = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];
    let extent = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2])
        .max(0.001);
    let scale = (width.min(height) as f32 * 0.8) / extent;

    let cos_yaw = yaw.cos();
    let sin_yaw = yaw.sin();
    let cos_pitch = pitch.cos();
    let sin_pitch = pitch.sin();

    let project = |position: [f32; 3]| -> (f32, f32, f32) {
        let mut p = [
            position[0] - centre[0],
            position[1] - centre[1],
            position[2] - centre[2],
        ];
        // Rotate around the vertical axis, then around the horizontal one.
        let x = p[0] * cos_yaw - p[2] * sin_yaw;
        let z = p[0] * sin_yaw + p[2] * cos_yaw;
        p[0] = x;
        p[2] = z;
        let y = p[1] * cos_pitch - p[2] * sin_pitch;
        let z2 = p[1] * sin_pitch + p[2] * cos_pitch;
        p[1] = y;
        p[2] = z2;

        let screen_x = width as f32 / 2.0 + p[0] * scale;
        let screen_y = height as f32 / 2.0 - p[1] * scale;
        (screen_x, screen_y, p[2])
    };

    for quad in &mesh.quads {
        let projected: Vec<(f32, f32, f32)> = quad
            .corners
            .iter()
            .map(|corner| project(corner.position))
            .collect();
        for triangle in [(0, 1, 2), (0, 2, 3)] {
            draw_triangle(
                &mut image,
                &mut depth,
                texture,
                &quad.corners,
                &projected,
                triangle,
                quad.shade,
            );
        }
    }

    image
}

/// Fill a triangle with a depth buffer.
fn draw_triangle(
    image: &mut RgbaImage,
    depth: &mut [f32],
    texture: &RgbaImage,
    corners: &[Vertex; 4],
    projected: &[(f32, f32, f32)],
    triangle: (usize, usize, usize),
    shade: f32,
) {
    let (a, b, c) = (
        projected[triangle.0],
        projected[triangle.1],
        projected[triangle.2],
    );

    let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    if area.abs() < 1e-6 {
        return;
    }

    let min_x = a.0.min(b.0).min(c.0).floor().max(0.0) as u32;
    let max_x = (a.0.max(b.0).max(c.0).ceil() as i64).clamp(0, image.width() as i64 - 1) as u32;
    let min_y = a.1.min(b.1).min(c.1).floor().max(0.0) as u32;
    let max_y = (a.1.max(b.1).max(c.1).ceil() as i64).clamp(0, image.height() as i64 - 1) as u32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let w0 = ((b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0)) / area;
            let w1 = ((px - a.0) * (c.1 - a.1) - (py - a.1) * (c.0 - a.0)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }

            let z = w0 * a.2 + w1 * b.2 + w2 * c.2;
            let index = (y as usize) * (image.width() as usize) + x as usize;
            if z >= depth[index] {
                continue;
            }
            depth[index] = z;

            // The texture coordinates of the three corners.
            let uv0 = corners[triangle.0].uv;
            let uv1 = corners[triangle.1].uv;
            let uv2 = corners[triangle.2].uv;
            let u = w0 * uv0[0] + w1 * uv1[0] + w2 * uv2[0];
            let v = w0 * uv0[1] + w1 * uv1[1] + w2 * uv2[1];
            let pixel = sample_texture(texture, u, v);
            let shaded = Rgba([
                (pixel.0[0] as f32 * shade) as u8,
                (pixel.0[1] as f32 * shade) as u8,
                (pixel.0[2] as f32 * shade) as u8,
                pixel.0[3],
            ]);
            image.put_pixel(x, y, shaded);
        }
    }
}

/// Sample the texture with nearest neighbour.
fn sample_texture(texture: &RgbaImage, u: f32, v: f32) -> Rgba<u8> {
    if texture.width() == 0 || texture.height() == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let x = (u.round() as i64).clamp(0, texture.width() as i64 - 1) as u32;
    let y = (v.round() as i64).clamp(0, texture.height() as i64 - 1) as u32;
    *texture.get_pixel(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_model;
    use serde_json::json;

    fn cube_model() -> ModelFaces {
        parse_model(&json!({
            "texture_size": [64, 64],
            "parts": [{
                "name": "body",
                "cubes": [{
                    "origin": [-4.0, 0.0, -2.0],
                    "size": [8.0, 12.0, 4.0],
                    "uv": [16.0, 16.0]
                }]
            }]
        }))
        .unwrap()
    }

    #[test]
    fn builds_a_quad_for_every_face() {
        let mesh = build_mesh(&cube_model());
        assert_eq!(mesh.quads.len(), 6);
    }

    #[test]
    fn renders_something() {
        let mut texture = RgbaImage::new(64, 64);
        for (x, y, pixel) in texture.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
        }
        let mesh = build_mesh(&cube_model());
        let image = render(&mesh, &texture, 128, 128, 0.6, 0.3);
        assert_eq!(image.dimensions(), (128, 128));
        let drawn = image.pixels().filter(|pixel| pixel.0[3] > 0).count();
        assert!(drawn > 1000, "the model should cover a good part of the image");
    }

    #[test]
    fn an_empty_model_renders_nothing() {
        let image = render(&Mesh::default(), &RgbaImage::new(16, 16), 32, 32, 0.0, 0.0);
        assert!(image.pixels().all(|pixel| pixel.0[3] == 0));
    }
}
