#!/usr/bin/env python3
"""Debug the java extractor: print the extracted model."""
p = "src/java.rs"
s = open(p).read()
old = """    #[test]
    fn debug_methods() {
        for (name, body) in methods_of(COW) {
            println!("METHOD {name}: {}", &body[..body.len().min(60)]);
        }
    }"""
new = """    #[test]
    fn debug_methods() {
        for model in extract_file(COW, "CowModel.java") {
            println!("MODEL {}", serde_json::to_string_pretty(&model).unwrap());
        }
    }"""
assert old in s
s = s.replace(old, new)
open(p, "w").write(s)
print("debug test updated")
