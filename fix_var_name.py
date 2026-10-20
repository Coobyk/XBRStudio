#!/usr/bin/env python3
"""Fix the variable name extraction and drop the debug prints."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """            let name = before[..index]
                .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();"""
new = """            let name = before[..index]
                .trim()
                .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();"""
assert old in s
s = s.replace(old, new)

old_debug = """            eprintln!(
                "DEBUG call={call_index} name={name} receiver={receiver:?} assigned={assigned:?} parent={parent:?} vars={variables:?}"
            );
"""
assert old_debug in s
s = s.replace(old_debug, "")

open(p, "w").write(s)
print("variable name extraction fixed")
