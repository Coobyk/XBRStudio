#!/usr/bin/env python3
"""Mention the pack folder in the README."""
import sys

p = "README.md"
s = open(p).read()

old = """```sh
# a whole resource pack
xbrstudio --jar minecraft-client.jar --output pack_x4 --zip pack_x4.zip
```"""
new = """```sh
# a whole resource pack
xbrstudio --jar minecraft-client.jar --output pack_x4 --zip pack_x4.zip

# an existing resource pack, a zip or a folder
xbrstudio --jar my-pack.zip --output pack_x4
```"""
assert old in s
s = s.replace(old, new)

old = """The tool upscales every texture under `assets/minecraft/textures/`:"""
new = """The source can be a client jar, a resource pack zip or a resource pack
folder — the layout is the same, so an existing pack is upscaled the same
way.

The tool upscales every texture under `assets/minecraft/textures/`:"""
assert old in s
s = s.replace(old, new)

open(p, "w").write(s)
print("README updated")
