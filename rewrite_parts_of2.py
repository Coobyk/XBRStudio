#!/usr/bin/env python3
"""Rewrite parts_of() properly: variable parents, chained children, tree build."""
import sys

p = "src/java.rs"
s = open(p).read()

start = s.index("/// The parts of a model body, with their children.")
end = s.index("/// The cubes of a `CubeListBuilder` chain.")

new_block = r'''/// One `addOrReplaceChild` call.
struct PartCall {
    name: String,
    cubes: Vec<ExtractedCube>,
    translation: Vec<Value>,
    rotation: Vec<Value>,
    /// The part this one is a child of.
    parent: Option<usize>,
}

/// The parts of a model body, with their children.
///
/// Mojang's code builds a model in two styles and both are supported:
///
/// ```java
/// // chained
/// meshdefinition.getRoot().addOrReplaceChild("head", cubes, PartPose.ZERO)
///         .addOrReplaceChild("nose", cubes, PartPose.ZERO);
/// // through a variable
/// PartDefinition partdefinition = meshdefinition.getRoot()
///         .addOrReplaceChild("head", cubes, PartPose.ZERO);
/// partdefinition.addOrReplaceChild("nose", cubes, PartPose.ZERO);
/// ```
fn parts_of(body: &str) -> Vec<ExtractedPart> {
    let mut calls: Vec<PartCall> = Vec::new();
    // Variable name to the part it holds.
    let mut variables: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for statement in split_statements(body) {
        let positions = find_calls(&statement, "addOrReplaceChild(");
        if positions.is_empty() {
            continue;
        }

        // The receiver of the first call in the statement says where the
        // part is attached, and the same statement may assign it to a
        // variable.
        let before = &statement[..positions[0].0];
        let equals = before.rfind('=');
        let receiver = match equals {
            Some(index) => before[index + 1..].trim(),
            None => before.trim(),
        };
        let assigned = equals.and_then(|index| {
            let name = before[..index]
                .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty()
                || !name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                None
            } else {
                Some(name)
            }
        });

        let mut previous: Option<usize> = None;
        for (call_index, (start, end)) in positions.iter().copied().enumerate() {
            let open = start + "addOrReplaceChild".len();
            let args = split_args(&statement[open + 1..end]);

            let name = args
                .first()
                .and_then(|arg| parse_string(arg))
                .unwrap_or_else(|| format!("part{}", calls.len()));
            let cubes = args.get(1).map(|arg| cubes_of(arg)).unwrap_or_default();
            let pose = args.get(2).map(|arg| pose_of(arg)).unwrap_or_default();

            let parent = if call_index == 0 {
                parent_of_receiver(receiver, &variables)
            } else {
                previous
            };

            calls.push(PartCall {
                name,
                cubes,
                translation: pose.0,
                rotation: pose.1,
                parent,
            });
            previous = Some(calls.len() - 1);
        }

        if let (Some(variable), Some(index)) = (assigned, previous) {
            variables.insert(variable, index);
        }
    }

    // Build the tree. Children always come after their parent, so the parts
    // are assembled back to front.
    let mut children: Vec<Vec<ExtractedPart>> = vec![Vec::new(); calls.len()];
    let mut roots: Vec<ExtractedPart> = Vec::new();
    for index in (0..calls.len()).rev() {
        let call = &calls[index];
        let part = ExtractedPart {
            name: call.name.clone(),
            translation: call.translation.clone(),
            rotation: call.rotation.clone(),
            cubes: call.cubes.clone(),
            children: std::mem::take(&mut children[index]),
        };
        match call.parent {
            Some(parent) if parent < children.len() => children[parent].push(part),
            _ => roots.push(part),
        }
    }
    roots.reverse();
    for child_list in children.iter_mut() {
        child_list.reverse();
    }
    roots
}

/// Split a Java body into statements (semicolon separated, at bracket depth 0).
fn split_statements(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut statements = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' | b'\'' => {
                index = skip_string(body, index);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b';' if depth == 0 => {
                statements.push(body[start..index].to_string());
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if start < body.len() {
        statements.push(body[start..].to_string());
    }
    statements
}

/// The positions of every `name(` call in a statement.
fn find_calls(statement: &str, name: &str) -> Vec<(usize, usize)> {
    let mut calls = Vec::new();
    let mut search = 0usize;
    while let Some(found) = statement[search..].find(name) {
        let open = search + found + name.len() - 1;
        let Some(close) = matching_delimiter(statement, open, b'(', b')') else {
            break;
        };
        calls.push((search + found, close));
        search = close + 1;
    }
    calls
}

/// The part a receiver expression refers to.
fn parent_of_receiver(
    receiver: &str,
    variables: &std::collections::HashMap<String, usize>,
) -> Option<usize> {
    let receiver = receiver.trim().trim_end_matches('.').trim();
    if receiver.is_empty() {
        return None;
    }
    // A plain variable name.
    if receiver
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    {
        return variables.get(receiver).copied();
    }
    None
}

'''

s = s[:start] + new_block + s[end:]
open(p, "w").write(s)
print("parts_of rewritten")
