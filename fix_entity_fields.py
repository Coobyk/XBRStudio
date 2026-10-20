#!/usr/bin/env python3
"""Make the fields of the entity model public."""
import re
import sys

p = "src/jar/entities.rs"
s = open(p).read()

# The fields of EntityModel and TextureCtx.
for struct in ["EntityModel", "TextureCtx"]:
    pattern = re.compile(rf"(pub struct {struct} \{{)(.*?)(\n\}})", re.S)
    match = pattern.search(s)
    if not match:
        print("NOT FOUND", struct)
        sys.exit(1)
    body = match.group(2)
    fixed = re.sub(r"^    (?!pub |#)([a-z_]+:)", r"    pub \1", body, flags=re.M)
    s = s[: match.start()] + match.group(1) + fixed + match.group(3) + s[match.end() :]

open(p, "w").write(s)
print("entity fields are public")
