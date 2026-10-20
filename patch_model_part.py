#!/usr/bin/env python3
"""Keep the part name in the model faces."""
import re
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
    """    /// The size of the cube the face belongs to (entity models only).
    pub size: Option<[f32; 3]>,
}""",
    """    /// The size of the cube the face belongs to (entity models only).
    pub size: Option<[f32; 3]>,
    /// The name of the part the face belongs to (entity models only).
    pub part: Option<String>,
}""",
)

swap(
    """fn collect_entity_faces(
    part: &RawEntityPart,
    textures: &HashMap<String, String>,
    out: &mut Vec<ModelFace>,
    group_counter: &mut u32,
) {""",
    """fn collect_entity_faces(
    part: &RawEntityPart,
    textures: &HashMap<String, String>,
    out: &mut Vec<ModelFace>,
    group_counter: &mut u32,
) {
    collect_entity_faces_named(part, textures, out, group_counter, None);
}

fn collect_entity_faces_named(
    part: &RawEntityPart,
    textures: &HashMap<String, String>,
    out: &mut Vec<ModelFace>,
    group_counter: &mut u32,
    parent: Option<&str>,
) {
    let part_name = parent.map(|name| name.to_string()).or_else(|| {
        part.name.clone()
    });""",
)

swap(
    """                group,
                origin: Some(cube.origin),
                size: Some(cube.size),
            });""",
    """                group,
                origin: Some(cube.origin),
                size: Some(cube.size),
                part: part_name.clone(),
            });""",
)

swap(
    """    for child in &part.children {
        collect_entity_faces(child, textures, out, group_counter);
    }
}""",
    """    for child in &part.children {
        let name = child.name.clone();
        collect_entity_faces_named(child, textures, out, group_counter, name.as_deref());
    }
}""",
)

swap(
    """#[derive(Deserialize)]
struct RawEntityPart {
    #[serde(default)]
    cubes: Vec<RawEntityCube>,
    #[serde(default)]
    children: Vec<RawEntityPart>,
}""",
    """#[derive(Deserialize)]
struct RawEntityPart {
    #[serde(default)]
    name: String,
    #[serde(default)]
    cubes: Vec<RawEntityCube>,
    #[serde(default)]
    children: Vec<RawEntityPart>,
}""",
)

# The block model faces have no part.
swap(
    """                face: face_name,
                group: element_index as u32,
                origin: None,
                size: None,
            });""",
    """                face: face_name,
                group: element_index as u32,
                origin: None,
                size: None,
                part: None,
            });""",
)

open(p, "w").write(s)
print("model.rs keeps the part names")

# The other files need the new field in their literals.
for path in ["src/upscaling.rs", "src/jar.rs", "src/bin/visualize.rs", "src/bin/slice.rs"]:
    try:
        with open(path, "r", encoding="utf-8") as handle:
            source = handle.read()
    except FileNotFoundError:
        continue
    pattern = re.compile(r"(size: None,\n)(\s*)(\})")
    updated, count = pattern.subn(
        lambda match: match.group(1)
        + f"{match.group(2)}part: None,\n"
        + match.group(3),
        source,
    )
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(updated)
    if count:
        print(f"{path}: {count} literals updated")
