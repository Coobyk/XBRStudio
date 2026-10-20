#!/usr/bin/env python3
"""Keep the cube origin and size in the model faces."""
import sys

p = "src/model.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:300])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:300]))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """#[derive(Debug, Clone)]
pub struct ModelFace {
    #[allow(dead_code)]
    pub texture: String,
    pub rect: FaceRect,
    pub face: String,
    /// Cube/element index within the model; neighbors are only looked up
    /// inside this group.
    pub group: u32,
}""",
    """#[derive(Debug, Clone)]
pub struct ModelFace {
    #[allow(dead_code)]
    pub texture: String,
    pub rect: FaceRect,
    pub face: String,
    /// Cube/element index within the model; neighbors are only looked up
    /// inside this group.
    pub group: u32,
    /// The corner of the cube the face belongs to (entity models only).
    pub origin: Option<[f32; 3]>,
    /// The size of the cube the face belongs to (entity models only).
    pub size: Option<[f32; 3]>,
}""",
)

swap(
    """#[derive(Deserialize)]
struct RawEntityCube {
    #[serde(default)]
    size: [f32; 3],
    #[serde(default)]
    uv: [f32; 2],
}""",
    """#[derive(Deserialize)]
struct RawEntityCube {
    #[serde(default)]
    origin: [f32; 3],
    #[serde(default)]
    size: [f32; 3],
    #[serde(default)]
    uv: [f32; 2],
}""",
)

swap(
    """            out.push(ModelFace {
                texture: first_texture_reference(textures).unwrap_or_default(),
                rect: FaceRect {
                    x: x.round().max(0.0) as u32,
                    y: y.round().max(0.0) as u32,
                    width: w.round().max(0.0) as u32,
                    height: h.round().max(0.0) as u32,
                },
                face: face_name.into(),
                group,
            });""",
    """            out.push(ModelFace {
                texture: first_texture_reference(textures).unwrap_or_default(),
                rect: FaceRect {
                    x: x.round().max(0.0) as u32,
                    y: y.round().max(0.0) as u32,
                    width: w.round().max(0.0) as u32,
                    height: h.round().max(0.0) as u32,
                },
                face: face_name.into(),
                group,
                origin: Some(cube.origin),
                size: Some(cube.size),
            });""",
)

# The block model faces and the test fixtures need the new fields.
swap(
    """            faces.push(ModelFace {
                texture,
                rect: FaceRect {
                    x: uv[0].round() as u32,
                    y: uv[1].round() as u32,
                    width: (uv[2] - uv[0]).round().max(0.0) as u32,
                    height: (uv[3] - uv[1]).round().max(0.0) as u32,
                },
                face: face_name,
                group: element_index as u32,
            });""",
    """            faces.push(ModelFace {
                texture,
                rect: FaceRect {
                    x: uv[0].round() as u32,
                    y: uv[1].round() as u32,
                    width: (uv[2] - uv[0]).round().max(0.0) as u32,
                    height: (uv[3] - uv[1]).round().max(0.0) as u32,
                },
                face: face_name,
                group: element_index as u32,
                origin: None,
                size: None,
            });""",
)

# Test fixtures.
swap(
    """                ModelFace {
                    texture: String::new(),
                    rect: FaceRect {
                        x: 0,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    face: "north".into(),
                    group: 0,
                },""",
    """                ModelFace {
                    texture: String::new(),
                    rect: FaceRect {
                        x: 0,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    face: "north".into(),
                    group: 0,
                    origin: None,
                    size: None,
                },""",
)

swap(
    """                ModelFace {
                    texture: String::new(),
                    rect: FaceRect {
                        x: 16,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    face: "east".into(),
                    group: 1,
                },""",
    """                ModelFace {
                    texture: String::new(),
                    rect: FaceRect {
                        x: 16,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    face: "east".into(),
                    group: 1,
                    origin: None,
                    size: None,
                },""",
)

swap(
    """            faces: vec![ModelFace {
                texture: String::new(),
                rect: FaceRect {
                    x: 80,
                    y: 0,
                    width: 16,
                    height: 16,
                },
                face: "north".into(),
                group: 0,
            }],""",
    """            faces: vec![ModelFace {
                texture: String::new(),
                rect: FaceRect {
                    x: 80,
                    y: 0,
                    width: 16,
                    height: 16,
                },
                face: "north".into(),
                group: 0,
                origin: None,
                size: None,
            }],""",
)

swap(
    """        let full = ModelFace {
            texture: "all".into(),
            rect: FaceRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            face: "north".into(),
            group: 0,
        };""",
    """        let full = ModelFace {
            texture: "all".into(),
            rect: FaceRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            face: "north".into(),
            group: 0,
            origin: None,
            size: None,
        };""",
)

open(p, "w").write(s)
print("model.rs keeps the cube geometry")
