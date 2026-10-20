#!/usr/bin/env python3
"""Fix the child detection in the java extractor and drop the debug test."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """    // Rebuild the tree from the flat list.
    let mut roots: Vec<ExtractedPart> = Vec::new();
    let mut stack: Vec<Vec<usize>> = Vec::new();
    for (part, chained) in flat {
        if chained && !stack.is_empty() {"""
new = """    // Rebuild the tree from the flat list. The flag of a part says that the
    // *next* part is one of its children, so it is looked up one step back.
    let mut roots: Vec<ExtractedPart> = Vec::new();
    let mut stack: Vec<Vec<usize>> = Vec::new();
    for (index, (part, next_is_child)) in flat.into_iter().enumerate() {
        let chained = index > 0 && flat_flags[index - 1];
        let _ = next_is_child;
        if chained && !stack.is_empty() {"""
assert old in s, "tree block not found"
s = s.replace(old, new)

# Collect the flags separately so the previous one can be looked up.
old_flat = """    let mut flat: Vec<(ExtractedPart, bool)> = Vec::new();
    let mut search = 0usize;"""
new_flat = """    let mut flat: Vec<(ExtractedPart, bool)> = Vec::new();
    let mut flat_flags: Vec<bool> = Vec::new();
    let mut search = 0usize;"""
assert old_flat in s
s = s.replace(old_flat, new_flat)

old_push = """        flat.push((part, chained));
        search = close + 1;"""
new_push = """        flat.push((part, chained));
        flat_flags.push(chained);
        search = close + 1;"""
assert old_push in s
s = s.replace(old_push, new_push)

old_loop = """    for (index, (part, next_is_child)) in flat.into_iter().enumerate() {
        let chained = index > 0 && flat_flags[index - 1];
        let _ = next_is_child;
        if chained && !stack.is_empty() {"""
new_loop = """    for (index, (part, _)) in flat.into_iter().enumerate() {
        let chained = index > 0 && flat_flags[index - 1];
        if chained && !stack.is_empty() {"""
assert old_loop in s
s = s.replace(old_loop, new_loop)

# Drop the debug test.
old_debug = """    #[test]
    fn debug_methods() {
        for model in extract_file(COW, "CowModel.java") {
            println!("MODEL {}", serde_json::to_string_pretty(&model).unwrap());
        }
    }

"""
assert old_debug in s
s = s.replace(old_debug, "")

open(p, "w").write(s)
print("java.rs fixed")
