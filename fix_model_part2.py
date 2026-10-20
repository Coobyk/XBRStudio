#!/usr/bin/env python3
"""Fix the part name handling in the model."""
import re
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
    """    let part_name = parent.map(|name| name.to_string()).or_else(|| {
        part.name.clone()
    });""",
    """    let part_name = parent
        .map(|name| name.to_string())
        .or_else(|| Some(part.name.clone()));""",
)

swap(
    """        let name = child.name.clone();
        collect_entity_faces_named(child, textures, out, group_counter, name.as_deref());""",
    """        let name = child.name.clone();
        collect_entity_faces_named(
            child,
            textures,
            out,
            group_counter,
            Some(name.as_str()),
        );""",
)

# The test fixtures need the new field.
pattern = re.compile(r"(size: None,\n)(\s*)(\})")
s, count = pattern.subn(
    lambda match: match.group(1) + f"{match.group(2)}part: None,\n" + match.group(3),
    s,
)
print(f"model.rs: {count} literals updated")

open(p, "w").write(s)
print("model.rs fixed")
