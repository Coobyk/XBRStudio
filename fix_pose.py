#!/usr/bin/env python3
"""Keep unresolvable pose expressions as text."""
import sys

p = "src/java.rs"
s = open(p).read()

start = s.index("/// The translation and the rotation of a `PartPose`.")
end = s.index("fn numbers_in(text: &str) -> Vec<f32> {")

new_block = r'''/// The translation and the rotation of a `PartPose`.
///
/// Values that are not plain numbers are kept as text, so a rotation that
/// depends on a variable is not silently lost.
fn pose_of(expression: &str) -> (Vec<Value>, Vec<Value>) {
    let text = expression.trim();
    let zero = vec![json!(0.0), json!(0.0), json!(0.0)];

    if text.contains("PartPose.ZERO") {
        return (zero.clone(), zero);
    }
    if let Some(args) = call_arguments(text, "PartPose.offset(") {
        return (pose_values(&args), zero);
    }
    if let Some(args) = call_arguments(text, "PartPose.rotationDegrees(") {
        return (zero, pose_values(&args));
    }
    if let Some(args) = call_arguments(text, "PartPose.rotation(") {
        return (zero, pose_values(&args));
    }
    (zero.clone(), zero)
}

/// The arguments of a call inside an expression.
fn call_arguments(text: &str, call: &str) -> Option<Vec<String>> {
    let start = text.find(call)? + call.len() - 1;
    let close = matching_delimiter(text, start, b'(', b')')?;
    Some(split_args(&text[start + 1..close]))
}

/// Numbers stay numbers, expressions are kept as text.
fn pose_values(args: &[String]) -> Vec<Value> {
    args.iter()
        .take(3)
        .map(|arg| match parse_number(arg) {
            Some(number) => json!(number),
            None => json!(arg.trim()),
        })
        .collect()
}

'''

s = s[:start] + new_block + s[end:]
open(p, "w").write(s)
print("pose_of improved")
