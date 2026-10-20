#!/usr/bin/env python3
"""Add model listing, model loading and cancellation to the batch."""
import sys

p = "src/jar.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:300])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:300]))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """    /// Also write OptiFine connected textures for blocks with several
    /// different faces.
    pub connected_textures: bool,
}""",
    """    /// Also write OptiFine connected textures for blocks with several
    /// different faces.
    pub connected_textures: bool,
    /// Stop the batch when this flag is set.
    pub cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}""",
)

swap(
    """            pack: PackOptions::default(),
            connected_textures: false,
        }
    }
}""",
    """            pack: PackOptions::default(),
            connected_textures: false,
            cancel: None,
        }
    }
}""",
)

# The cancellation check inside the worker.
swap(
    """            let outcome = match &cache {""",
    """            if let Some(cancel) = &opts.cancel
                && cancel.load(Ordering::Relaxed)
            {
                return (
                    job.name.clone(),
                    Ok(ProcessOutcome {
                        animated: false,
                        stitched: false,
                        wrapped: false,
                        copied: false,
                        skipped: true,
                        connected: false,
                    }),
                );
            }
            let outcome = match &cache {""",
)

# Public helpers for the model picker.
swap(
    """fn load_block_models(""",
    """/// The names of the block models in a jar, sorted.
pub fn list_block_models(jar_path: &Path) -> Result<Vec<String>, String> {
    let file = File::open(jar_path)
        .map_err(|error| format!("cannot open {}: {error}", jar_path.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("cannot read jar: {error}"))?;
    let mut names: Vec<String> = Vec::new();
    for index in 0..archive.len() {
        let name = archive
            .by_index(index)
            .map_err(|error| format!("cannot list jar entry: {error}"))?
            .name()
            .to_string();
        if name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json") {
            let short = name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if !short.is_empty() {
                names.push(short);
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

/// The faces of one block model from a jar, with the parent chain resolved.
pub fn load_block_model(jar_path: &Path, name: &str) -> Result<ModelFaces, String> {
    let file = File::open(jar_path)
        .map_err(|error| format!("cannot open {}: {error}", jar_path.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("cannot read jar: {error}"))?;

    let mut raw: HashMap<String, Value> = HashMap::new();
    let mut index_by_name: HashMap<String, usize> = HashMap::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("cannot list jar entry: {error}"))?;
        let entry_name = entry.name().to_string();
        index_by_name.insert(entry_name.clone(), index);
        if entry_name.starts_with(BLOCK_MODEL_ROOT) && entry_name.ends_with(".json") {
            let short = entry_name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if short.is_empty() {
                continue;
            }
            let bytes = read_entry(&mut archive, index)?;
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                raw.insert(short, value);
            }
        }
    }

    let resolved = resolve_block_models(&raw);
    let value = resolved
        .get(name)
        .ok_or_else(|| format!("no block model named {name}"))?;
    parse_model(value)
}

fn load_block_models(""",
)

open(p, "w").write(s)
print("jar.rs helpers added")
