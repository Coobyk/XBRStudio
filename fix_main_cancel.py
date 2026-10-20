#!/usr/bin/env python3
"""Add the cancel field to the CLI batch options."""
import sys

p = "src/main.rs"
s = open(p).read()

old = """        pack: pack_options,
        connected_textures: cli.connected_textures
            || config.connected_textures.unwrap_or(false),
    };"""
new = """        pack: pack_options,
        connected_textures: cli.connected_textures
            || config.connected_textures.unwrap_or(false),
        cancel: None,
    };"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("main.rs fixed")
