//! Extract Minecraft entity models from decompiled Java sources.
//!
//! Modern Minecraft defines entity models in code, not in JSON: a model layer
//! is a `LayerDefinition` built from `CubeListBuilder` chains. This module
//! reads such a source file and turns it into the model JSON the stitcher
//! understands.
//!
//! The parser is deliberately tolerant: it looks for the shapes Mojang's
//! decompilation actually uses (`addOrReplaceChild`, `texOffs`, `addBox`,
//! `PartPose`) and ignores everything else. Expressions it cannot evaluate
//! (a deformation parameter, a rotation that depends on a variable) are kept
//! as text so nothing is silently lost.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

/// One cube of a model part.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedCube {
    pub origin: [f32; 3],
    pub size: [f32; 3],
    pub uv: [f32; 2],
    /// The deformation expression, kept as written in the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deform: Option<String>,
    #[serde(default)]
    pub mirror: bool,
}

/// One part of a model.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedPart {
    pub name: String,
    pub translation: Vec<Value>,
    pub rotation: Vec<Value>,
    #[serde(default)]
    pub cubes: Vec<ExtractedCube>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ExtractedPart>,
}

/// A model layer extracted from one method.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedModel {
    /// Fully qualified class name.
    pub model: String,
    /// The method the model was built in.
    pub method: String,
    /// `texture_size` of the layer, when the method returns a
    /// `LayerDefinition`.
    pub texture_size: Option<[f32; 2]>,
    pub parts: Vec<ExtractedPart>,
}

/// Extract every model from one Java source file.
///
/// `file_name` is the file name (`CowModel.java`); the package comes from the
/// source itself.
pub fn extract_file(source: &str, file_name: &str) -> Vec<ExtractedModel> {
    let package = package_of(source);
    let class_name = file_name
        .strip_suffix(".java")
        .unwrap_or(file_name)
        .to_string();
    let fqcn = match &package {
        Some(package) if !package.is_empty() => format!("{package}.{class_name}"),
        _ => class_name.clone(),
    };

    let mut models = Vec::new();
    for (method, body) in methods_of(source) {
        if !body.contains("addOrReplaceChild") && !body.contains("CubeListBuilder") {
            continue;
        }
        let texture_size = texture_size_of(&body);
        let parts = parts_of(&body);
        if parts.is_empty() {
            continue;
        }
        models.push(ExtractedModel {
            model: fqcn.clone(),
            method,
            texture_size,
            parts,
        });
    }
    models
}

/// Extract every model from a directory of Java sources.
///
/// Returns the models together with the path they should be written to,
/// relative to the output directory: the package as directories and the
/// file name `<Class>.<method>.json`.
pub fn extract_tree(root: &Path) -> Vec<(PathBuf, ExtractedModel)> {
    let mut out = Vec::new();
    let mut files = Vec::new();
    collect_java_files(root, &mut files);
    files.sort();

    for file in files {
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Some(name) = file.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        for model in extract_file(&source, name) {
            let class_name = model
                .model
                .rsplit('.')
                .next()
                .unwrap_or(&model.model)
                .to_string();
            let mut path = PathBuf::new();
            if let Some((package, _)) = model.model.rsplit_once('.') {
                for part in package.split('.') {
                    path.push(part);
                }
            }
            path.push(format!("{class_name}.{}.json", model.method));
            out.push((path, model));
        }
    }
    out
}

/// Write the extracted models as JSON files.
pub fn write_models(models: &[(PathBuf, ExtractedModel)], out_dir: &Path) -> Result<usize, String> {
    let mut written = 0;
    for (path, model) in models {
        let target = out_dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        let data = serde_json::to_vec_pretty(model)
            .map_err(|error| format!("encode {}: {error}", target.display()))?;
        std::fs::write(&target, data)
            .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
        written += 1;
    }
    Ok(written)
}

fn collect_java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "java") {
            out.push(path);
        }
    }
}

fn package_of(source: &str) -> Option<String> {
    let index = source.find("package ")?;
    let rest = &source[index + "package ".len()..];
    let end = rest.find(';')?;
    Some(rest[..end].trim().to_string())
}

/// The methods of a Java file as `(name, body)` pairs.
fn methods_of(source: &str) -> Vec<(String, String)> {
    let bytes = source.as_bytes();
    let mut methods = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        // A method signature ends with `) {`.
        let Some(open) = source[index..].find('(') else {
            break;
        };
        let open = index + open;
        let Some(close) = matching_delimiter(source, open, b'(', b')') else {
            break;
        };
        let after = source[close + 1..].trim_start();
        if !after.starts_with('{') {
            index = close + 1;
            continue;
        }
        // Skip constructors and control keywords.
        let before = source[..open].trim_end();
        let name = before
            .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '<')
            .next()
            .unwrap_or_default()
            .to_string();
        let is_method = !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && !matches!(
                name.as_str(),
                "if" | "for" | "while" | "switch" | "catch" | "synchronized" | "return" | "new"
            );
        if !is_method {
            index = close + 1;
            continue;
        }
        // The body starts after the opening brace.
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
        index = body_end + 1;
    }

    methods
}

/// Index of the delimiter that closes the one at `open`.
fn matching_delimiter(source: &str, open: usize, open_char: u8, close_char: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' || byte == b'\'' {
            index = skip_string(source, index);
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index += 2;
            continue;
        }
        if byte == open_char {
            depth += 1;
        } else if byte == close_char {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn skip_string(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        if bytes[index] == quote {
            return index + 1;
        }
        index += 1;
    }
    index
}

/// `LayerDefinition.create(mesh, width, height)` — the texture size.
fn texture_size_of(body: &str) -> Option<[f32; 2]> {
    let mut search = 0usize;
    while let Some(found) = body[search..].find("LayerDefinition.create(") {
        let open = search + found + "LayerDefinition.create".len();
        let close = matching_delimiter(body, open, b'(', b')')?;
        let args = split_args(&body[open + 1..close]);
        let numbers: Vec<f32> = args.iter().filter_map(|arg| parse_number(arg)).collect();
        if numbers.len() >= 2 {
            return Some([numbers[numbers.len() - 2], numbers[numbers.len() - 1]]);
        }
        search = close;
    }
    None
}

/// One `addOrReplaceChild` call.
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
                .trim()
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

/// The cubes of a `CubeListBuilder` chain.
///
/// The chain is read in order, because every `texOffs` only applies to the
/// boxes that follow it:
///
/// ```java
/// CubeListBuilder.create().texOffs(0, 0)
///         .addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))
///         .texOffs(1, 33)
///         .addBox(-3.0F, 1.0F, -7.0F, 6.0F, 3.0F, 1.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))
/// ```
fn cubes_of(chain: &str) -> Vec<ExtractedCube> {
    let mut cubes = Vec::new();
    let mut uv = [0.0f32, 0.0f32];
    let mut search = 0usize;

    while search < chain.len() {
        let next_tex = chain[search..].find("texOffs(");
        let next_box = chain[search..].find("addBox(");

        // Whichever comes first.
        let take_tex = match (next_tex, next_box) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(tex), Some(box_)) => tex <= box_,
        };

        if take_tex {
            let open = search + next_tex.unwrap() + "texOffs".len();
            let Some(close) = matching_delimiter(chain, open, b'(', b')') else {
                break;
            };
            let args = split_args(&chain[open + 1..close]);
            let numbers: Vec<f32> = args.iter().filter_map(|arg| parse_number(arg)).collect();
            if numbers.len() >= 2 {
                uv = [numbers[0], numbers[1]];
            }
            search = close + 1;
            continue;
        }

        let open = search + next_box.unwrap() + "addBox".len();
        let Some(close) = matching_delimiter(chain, open, b'(', b')') else {
            break;
        };
        let args = split_args(&chain[open + 1..close]);
        let numbers: Vec<f32> = args.iter().filter_map(|arg| parse_number(arg)).collect();
        if numbers.len() >= 6 {
            let origin = [numbers[0], numbers[1], numbers[2]];
            let size = [numbers[3], numbers[4], numbers[5]];
            let deform = args
                .get(6)
                .map(|arg| arg.trim().to_string())
                .filter(|text| {
                    !text.is_empty()
                        && !text
                            .chars()
                            .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == ' ')
                });
            let mirror = args.get(7).map(|arg| arg.trim() == "true").unwrap_or(false);
            cubes.push(ExtractedCube {
                origin,
                size,
                uv,
                deform,
                mirror,
            });
        }
        search = close + 1;
    }

    cubes
}

/// The translation and the rotation of a `PartPose`.
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

/// Split a comma separated argument list, ignoring commas inside nested
/// calls, arrays and strings.
pub fn split_args(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' | b'\'' => {
                let end = skip_string(text, index);
                current.push_str(&text[index..end]);
                index = end;
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
                index += 1;
                continue;
            }
            _ => {}
        }
        current.push(byte as char);
        index += 1;
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

/// Parse a Java numeric literal.
pub fn parse_number(text: &str) -> Option<f32> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip a trailing type suffix.
    let mut end = trimmed.len();
    let bytes = trimmed.as_bytes();
    if bytes[end - 1].is_ascii_alphabetic() {
        end -= 1;
    }
    let candidate = &trimmed[..end];
    // Reject expressions such as `1.0F + yOffset`.
    if candidate.contains('+') || candidate.contains('*') || candidate.contains('/') {
        return None;
    }
    candidate.parse::<f32>().ok()
}

/// Parse a Java string literal.
pub fn parse_string(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return Some(trimmed[1..trimmed.len() - 1].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const COW: &str = r#"
package net.minecraft.client.model.animal.cow;

public class CowModel {
    public static LayerDefinition createBodyLayer() {
        MeshDefinition meshdefinition = new CubeListBuilder();
        PartDefinition partdefinition = meshdefinition.getRoot()
            .addOrReplaceChild("head", CubeListBuilder.create().texOffs(0, 0)
                .addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO)
            .addOrReplaceChild("nose", CubeListBuilder.create().texOffs(22, 0)
                .addBox(-1.0F, -1.0F, -1.0F, 2.0F, 3.0F, 1.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO);
        return LayerDefinition.create(meshdefinition, 64, 64);
    }
}
"#;

    #[test]
    fn extracts_a_model_with_its_children() {
        let models = extract_file(COW, "CowModel.java");
        assert_eq!(models.len(), 1);
        let model = &models[0];
        assert_eq!(
            model.model,
            "net.minecraft.client.model.animal.cow.CowModel"
        );
        assert_eq!(model.method, "createBodyLayer");
        assert_eq!(model.texture_size, Some([64.0, 64.0]));
        assert_eq!(model.parts.len(), 1);
        assert_eq!(model.parts[0].name, "head");
        assert_eq!(model.parts[0].cubes.len(), 1);
        assert_eq!(model.parts[0].cubes[0].origin, [-4.0, -4.0, -6.0]);
        assert_eq!(model.parts[0].cubes[0].size, [8.0, 8.0, 6.0]);
        assert_eq!(model.parts[0].cubes[0].uv, [0.0, 0.0]);
        assert_eq!(model.parts[0].children.len(), 1);
        assert_eq!(model.parts[0].children[0].name, "nose");
        assert_eq!(model.parts[0].children[0].cubes[0].uv, [22.0, 0.0]);
    }

    const COW_VARIABLE: &str = r#"
package net.minecraft.client.model.animal.cow;

public class CowModel {
    public static LayerDefinition createBodyLayer() {
        MeshDefinition meshdefinition = new MeshDefinition();
        PartDefinition partdefinition = meshdefinition.getRoot()
            .addOrReplaceChild("head", CubeListBuilder.create().texOffs(0, 0)
                .addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO);
        partdefinition.addOrReplaceChild("nose", CubeListBuilder.create().texOffs(22, 0)
            .addBox(-1.0F, -1.0F, -1.0F, 2.0F, 3.0F, 1.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO);
        PartDefinition partdefinition1 = meshdefinition.getRoot()
            .addOrReplaceChild("body", CubeListBuilder.create().texOffs(16, 16)
                .addBox(-4.0F, 0.0F, -2.0F, 8.0F, 12.0F, 4.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO);
        return LayerDefinition.create(meshdefinition, 64, 64);
    }
}
"#;

    #[test]
    fn finds_children_that_are_added_through_a_variable() {
        let models = extract_file(COW_VARIABLE, "CowModel.java");
        assert_eq!(models.len(), 1);
        let parts = &models[0].parts;
        assert_eq!(parts.len(), 2, "head and body are roots");
        assert_eq!(parts[0].name, "head");
        assert_eq!(parts[1].name, "body");
        assert_eq!(parts[0].children.len(), 1, "nose belongs to the head");
        assert_eq!(parts[0].children[0].name, "nose");
        assert_eq!(parts[0].children[0].cubes[0].uv, [22.0, 0.0]);
    }

    #[test]
    fn parses_java_numbers() {
        assert_eq!(parse_number("1.0F"), Some(1.0));
        assert_eq!(parse_number("-4.0F"), Some(-4.0));
        assert_eq!(parse_number(" 64 "), Some(64.0));
        assert_eq!(parse_number("0.0F + yOffset"), None);
        assert_eq!(parse_number("g"), None);
    }

    #[test]
    fn splits_arguments() {
        let args = split_args(
            "\"head\", CubeListBuilder.create().addBox(1.0F, 2.0F, 3.0F), PartPose.ZERO",
        );
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "\"head\"");
        assert!(args[1].contains("addBox"));
        assert_eq!(args[2], "PartPose.ZERO");
    }

    #[test]
    fn reads_the_texture_size_of_a_layer() {
        let body = "return LayerDefinition.create(meshdefinition, 64, 64);";
        assert_eq!(texture_size_of(body), Some([64.0, 64.0]));
        assert_eq!(texture_size_of("return null;"), None);
    }

    #[test]
    fn every_texoffs_applies_to_the_boxes_that_follow_it() {
        let cubes = cubes_of(
            "CubeListBuilder.create().texOffs(0, 0)\
             .addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))\
             .texOffs(1, 33)\
             .addBox(-3.0F, 1.0F, -7.0F, 6.0F, 3.0F, 1.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))",
        );
        assert_eq!(cubes.len(), 2);
        assert_eq!(cubes[0].uv, [0.0, 0.0]);
        assert_eq!(cubes[1].uv, [1.0, 33.0]);
        assert_eq!(cubes[1].origin, [-3.0, 1.0, -7.0]);
        assert_eq!(cubes[1].size, [6.0, 3.0, 1.0]);
    }

    #[test]
    fn keeps_unresolvable_expressions_as_text() {
        let cubes = cubes_of(
            "CubeListBuilder.create().texOffs(0, 0).addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, g.extend(0.5F))",
        );
        assert_eq!(cubes.len(), 1);
        assert_eq!(cubes[0].deform.as_deref(), Some("g.extend(0.5F)"));
    }
}
