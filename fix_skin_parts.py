#!/usr/bin/env python3
"""Use the part names in the skin converter."""
import sys

p = "src/skin.rs"
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
    """    // Group the faces of the model by the part they belong to.
    let mut groups: std::collections::HashMap<&str, Vec<&crate::model::ModelFace>> =
        std::collections::HashMap::new();
    for face in &model.faces {
        let Some(part) = part_of(&face.face) else {
            continue;
        };
        // The part name is not in the face, so the group is used instead.
        let _ = part;
        groups.entry("group").or_default().push(face);
    }
    let _ = groups;

    // The model faces carry the part in their group; the part names come
    // from the model structure, so the caller has to pass a model whose
    // faces are already grouped per part.
    for (group_index, group) in model.faces.iter().fold(
        std::collections::BTreeMap::<u32, Vec<&crate::model::ModelFace>>::new(),
        |mut map, face| {
            map.entry(face.group).or_default().push(face);
            map
        },
    ) {
        let Some(north) = group.iter().find(|face| face.face == "north") else {
            continue;
        };
        let Some(part) = part_name_of(model, group_index) else {
            continue;
        };
        let Some(region) = legacy_region(&parts, part, false) else {
            continue;
        };
        let source = crop(legacy, region);

        write_box(
            &mut output,
            &group,
            &source,
            part == "left_arm" || part == "left_leg",
        );
    }

    Ok(output)
}

/// The part a group of faces belongs to.
///
/// The model does not keep the part names, so the group is matched by the
/// order of the faces: the first group with a head is the head, and so on.
fn part_name_of(model: &ModelFaces, group: u32) -> Option<&'static str> {
    let _ = model;
    let _ = group;
    None
}""",
    """    // Group the faces of the model by the part they belong to.
    let mut groups: std::collections::BTreeMap<u32, Vec<&crate::model::ModelFace>> =
        std::collections::BTreeMap::new();
    for face in &model.faces {
        groups.entry(face.group).or_default().push(face);
    }

    for group in groups.values() {
        let Some(part) = group
            .iter()
            .find_map(|face| face.part.as_deref())
            .and_then(part_of)
        else {
            continue;
        };
        let Some(region) = legacy_region(&parts, part, false) else {
            continue;
        };
        let source = crop(legacy, region);

        write_box(
            &mut output,
            group,
            &source,
            part == "left_arm" || part == "left_leg",
        );
    }

    Ok(output)
}""",
)

swap(
    """/// The part of the model a legacy region belongs to.
fn part_of(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();""",
    """/// The part of the model a legacy region belongs to.
fn part_of(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();""",
)

# The hat layer is a part of its own in the model.
swap(
    """    if name.contains("hat") {
        return Some("hat");
    }""",
    """    if name.contains("hat") || name.contains("helmet") {
        return Some("hat");
    }""",
)

open(p, "w").write(s)
print("skin.rs uses the part names")
