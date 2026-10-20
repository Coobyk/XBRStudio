#!/usr/bin/env python3
"""Make the cube origin tolerant and remove the debug test."""
import sys

p = "src/model.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:400])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:400]))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """#[derive(Deserialize)]
struct RawEntityCube {
    #[serde(default)]
    origin: [f32; 3],
    #[serde(default)]
    size: [f32; 3],
    #[serde(default)]
    uv: [f32; 2],
}""",
    """#[derive(Deserialize)]
struct RawEntityCube {
    /// The corner of the cube. It can be an expression (the models are
    /// extracted from code), so it is kept as a value.
    #[serde(default)]
    origin: Vec<Value>,
    #[serde(default)]
    size: [f32; 3],
    #[serde(default)]
    uv: [f32; 2],
}

/// The numbers of a list of values, when they all are numbers.
fn numbers_of(values: &[Value]) -> Option<[f32; 3]> {
    if values.len() != 3 {
        return None;
    }
    let mut numbers = [0.0f32; 3];
    for (index, value) in values.iter().enumerate() {
        numbers[index] = value.as_f64()? as f32;
    }
    Some(numbers)
}""",
)

swap(
    """                origin: Some(cube.origin),
                size: Some(cube.size),""",
    """                origin: numbers_of(&cube.origin),
                size: Some(cube.size),""",
)

open(p, "w").write(s)
print("model.rs origin is tolerant")

# Remove the debug test from jar.rs.
p = "src/jar.rs"
s = open(p).read()
start = s.index("    #[test]\n    fn debug_skipped_models() {")
end = s.index("    use super::*;")
s = s[:start] + s[end:]
open(p, "w").write(s)
print("jar.rs debug test removed")
