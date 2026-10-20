#!/usr/bin/env python3
"""Print the parse errors of the skipped models."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """            let Ok(parsed) = parse_model(&value) else {
                println!("SKIP (parse) {}", path.display());
                continue;
            };"""
new = """            let parsed = match parse_model(&value) {
                Ok(parsed) => parsed,
                Err(error) => {
                    println!("SKIP (parse) {}: {error}", path.display());
                    continue;
                }
            };"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("debug test updated")
