#!/usr/bin/env python3
"""Add temporary debug output to parts_of."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """            let parent = if call_index == 0 {
                parent_of_receiver(receiver, &variables)
            } else {
                previous
            };"""
new = """            let parent = if call_index == 0 {
                parent_of_receiver(receiver, &variables)
            } else {
                previous
            };
            eprintln!(
                "DEBUG call={call_index} name={name} receiver={receiver:?} assigned={assigned:?} parent={parent:?} vars={variables:?}"
            );"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("debug prints added")
