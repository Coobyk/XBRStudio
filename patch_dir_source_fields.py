#!/usr/bin/env python3
"""Add the new fields to the directory source."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """    Ok(Source {
        jobs,
        block_model_entries,
        pack_format,
    })
}"""
new = """    Ok(Source {
        jobs,
        block_model_entries,
        item_model_entries,
        atlas_entries,
        pack_format,
    })
}"""
if old in s:
    s = s.replace(old, new)
    open(p, "w").write(s)
    print("directory source updated")
else:
    print("already updated")
