#!/usr/bin/env python3
"""Debug the java extractor: print the methods it finds."""
import sys

p = "src/java.rs"
s = open(p).read()
old = """    #[test]
    fn extracts_a_model_with_its_children() {"""
new = """    #[test]
    fn debug_methods() {
        for (name, body) in methods_of(COW) {
            println!("METHOD {name}: {}", &body[..body.len().min(60)]);
        }
    }

    #[test]
    fn extracts_a_model_with_its_children() {"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("debug test added")
