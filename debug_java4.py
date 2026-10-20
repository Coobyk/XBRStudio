#!/usr/bin/env python3
"""Print the extracted model for the variable style."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """    #[test]
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
    }"""

new = """    #[test]
    fn debug_statements() {
        for (name, body) in methods_of(COW_VARIABLE) {
            println!("METHOD {name}");
            for statement in split_statements(&body) {
                let trimmed = statement.trim();
                if trimmed.is_empty() {
                    continue;
                }
                println!("  STMT: {}", &trimmed[..trimmed.len().min(60)]);
                for (start, end) in find_calls(&statement, "addOrReplaceChild(") {
                    println!("    CALL args={:?}", &split_args(&statement[start + 18..end]));
                }
            }
        }
        for model in extract_file(COW_VARIABLE, "CowModel.java") {
            println!("PARTS: {}", serde_json::to_string(&model.parts).unwrap());
        }
    }"""

assert old in s
s = s.replace(old, new, 1)
open(p, "w").write(s)
print("debug test updated")
