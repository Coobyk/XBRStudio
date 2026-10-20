#!/usr/bin/env python3
"""Restore the files and add the ModelFace fields to the test fixtures."""
import re
import subprocess
import sys

FILES = ["src/upscaling.rs", "src/jar.rs", "src/bin/visualize.rs"]

subprocess.run(["git", "checkout", "--"] + FILES, check=True)

pattern = re.compile(r"(group: \d+,\n)(\s*)(\})")

for path in FILES:
    with open(path, "r", encoding="utf-8") as handle:
        source = handle.read()

    def replace(match):
        indent = match.group(2)
        return (
            match.group(1)
            + f"{indent}origin: None,\n"
            + f"{indent}size: None,\n"
            + match.group(3)
        )

    updated, count = pattern.subn(replace, source)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(updated)
    print(f"{path}: {count} literals updated")
