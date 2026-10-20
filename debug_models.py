#!/usr/bin/env python3
"""Find the entity models that are skipped."""
import sys

p = "src/jar.rs"
s = open(p).read()

old = """#[cfg(test)]
mod tests {"""
new = """#[cfg(test)]
mod tests {
    #[test]
    fn debug_skipped_models() {
        let dir = std::path::PathBuf::from("models/entity");
        let mut files = Vec::new();
        collect_json_files(&dir, &mut files);
        files.sort();
        for path in files {
            let Ok(data) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                println!("SKIP (json) {}", path.display());
                continue;
            };
            if !value.get("parts").is_some_and(|parts| parts.is_array()) {
                println!("SKIP (parts) {}", path.display());
                continue;
            }
            let Some(fqcn) = value.get("model").and_then(Value::as_str) else {
                println!("SKIP (model) {}", path.display());
                continue;
            };
            if model_key(fqcn).is_empty() {
                println!("SKIP (key) {}", path.display());
                continue;
            }
            let Ok(parsed) = parse_model(&value) else {
                println!("SKIP (parse) {}", path.display());
                continue;
            };
            if parsed.faces.is_empty() {
                println!("SKIP (faces) {}", path.display());
                continue;
            }
        }
    }
"""
assert old in s
s = s.replace(old, new, 1)
open(p, "w").write(s)
print("debug test added")
