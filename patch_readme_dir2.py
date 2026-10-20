#!/usr/bin/env python3
"""Mention the pack folder in the README."""
import sys

p = "README.md"
s = open(p).read()

line = "xbrstudio --jar minecraft-client.jar --output pack_x4 --zip pack_x4.zip"
if "my-pack.zip" not in s:
    s = s.replace(
        line,
        line
        + "\n\n# an existing resource pack, a zip or a folder\nxbrstudio --jar my-pack.zip --output pack_x4",
    )

anchor = "The tool upscales every texture under"
if "The source can be a client jar" not in s:
    s = s.replace(
        anchor,
        "The source can be a client jar, a resource pack zip or a resource pack\n"
        "folder \u2014 the layout is the same, so an existing pack is upscaled the\n"
        "same way.\n\n" + anchor,
    )

open(p, "w").write(s)
print("README updated")
