#!/usr/bin/env python3
"""Print the statements and calls the extractor sees."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """    #[test]
    fn finds_children_that_are_added_through_a_variable() {"""
new = """    #[test]
    fn debug_statements() {
        for statement in split_statements(COW_VARIABLE) {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            println!("STMT: {}", &trimmed[..trimmed.len().min(70)]);
            for (start, end) in find_calls(&statement, "addOrReplaceChild(") {
                println!("   CALL {}..{} args={:?}", start, end, &split_args(&statement[start + 18..end]));
            }
        }
    }

    #[test]
    fn finds_children_that_are_added_through_a_variable() {"""
assert old in s
s = s.replace(old, new, 1)
open(p, "w").write(s)
print("debug test added")
