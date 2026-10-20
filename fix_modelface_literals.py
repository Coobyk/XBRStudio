#!/usr/bin/env python3
"""Add the new ModelFace fields to every literal that lacks them."""
import re
import sys

FILES = ["src/upscaling.rs", "src/jar.rs", "src/model.rs", "src/bin/slice.rs", "src/bin/visualize.rs"]

for path in FILES:
    try:
        with open(path, "r", encoding="utf-8") as handle:
            source = handle.read()
    except FileNotFoundError:
        continue

    out = []
    index = 0
    changed = 0
    while True:
        match = re.search(r"ModelFace \{", source[index:])
        if not match:
            out.append(source[index:])
            break
        start = index + match.start()
        brace = index + match.end() - 1
        # Find the matching closing brace.
        depth = 0
        cursor = brace
        while cursor < len(source):
            if source[cursor] == "{":
                depth += 1
            elif source[cursor] == "}":
                depth -= 1
                if depth == 0:
                    break
            cursor += 1
        body = source[brace + 1 : cursor]
        out.append(source[index:brace + 1])
        if "origin:" not in body:
            # Insert the fields before the closing brace.
            indent = "                "
            match_indent = re.search(r"\n(\s*)\w", body)
            if match_indent:
                indent = match_indent.group(1)
            out.append(body.rstrip())
            out.append(f"\n{indent}origin: None,\n{indent}size: None,")
            changed += 1
        else:
            out.append(body)
        out.append("}")
        index = cursor + 1

    with open(path, "w", encoding="utf-8") as handle:
        handle.write("".join(out))
    if changed:
        print(f"{path}: {changed} literals updated")
