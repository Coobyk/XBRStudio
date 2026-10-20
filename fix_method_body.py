#!/usr/bin/env python3
"""Fix the method body extraction and drop the debug test."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """        let Some(body_end) = matching_delimiter(source, close + 1, b'{', b'}') else {
            index = close + 1;
            continue;
        };
        methods.push((name, source[close + 2..body_end].to_string()));
        index = body_end + 1;"""

new = """        // The body starts after the opening brace.
        let Some(offset) = source[close + 1..].find('{') else {
            index = close + 1;
            continue;
        };
        let brace = close + 1 + offset;
        let Some(body_end) = matching_delimiter(source, brace, b'{', b'}') else {
            index = close + 1;
            continue;
        };
        methods.push((name, source[brace + 1..body_end].to_string()));
        index = body_end + 1;"""

assert old in s
s = s.replace(old, new)

old_debug = """    #[test]
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
    }

"""
assert old_debug in s
s = s.replace(old_debug, "")

open(p, "w").write(s)
print("method body extraction fixed")
