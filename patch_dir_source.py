#!/usr/bin/env python3
"""Let the batch read a resource pack directory as well as a jar."""
import sys

p = "src/jar.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:400])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:400]))
        sys.exit(1)
    s = s.replace(old, new)


# The entry point: a jar, a zip or a directory.
swap(
    """pub fn upscale_jar(
    jar_path: &Path,
    out_dir: &Path,
    opts: &BatchOptions,
    progress: impl Fn(BatchProgress) + Sync,
) -> Result<BatchReport, String> {
    opts.upscale_config(false).validate()?;

    let file = File::open(jar_path)
        .map_err(|error| format!("cannot open {}: {error}", jar_path.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("cannot read jar: {error}"))?;

    let mut index_by_name: HashMap<String, usize> = HashMap::with_capacity(archive.len());
    let mut textures: Vec<(usize, String)> = Vec::new();
    for index in 0..archive.len() {
        let name = archive
            .by_index(index)
            .map_err(|error| format!("cannot list jar entry: {error}"))?
            .name()
            .to_string();
        index_by_name.insert(name.clone(), index);
        if name.starts_with(TEXTURE_ROOT) && name.ends_with(".png") {
            textures.push((index, name));
        }
    }
    textures.sort_by(|a, b| a.1.cmp(&b.1));
    if let Some(only) = opts.only.as_deref() {
        textures.retain(|(_, name)| name.contains(only));
    }
    if textures.is_empty() {
        return Err(format!(
            "no PNG textures found under {TEXTURE_ROOT} in {}",
            jar_path.display()
        ));
    }

    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("cannot create {}: {error}", out_dir.display()))?;

    let models = if opts.stitch {
        let model_dir = opts.model_dir.clone().or_else(default_model_dir);
        model_dir
            .map(|dir| load_entity_models(&dir))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let block_models = if opts.stitch {
        load_block_models(&mut archive, &index_by_name)
    } else {
        HashMap::new()
    };
    let pack_format = read_pack_format(&mut archive);

    // Load bytes up front so workers never share the ZipArchive.
    let mut jobs = Vec::with_capacity(textures.len());
    for (index, name) in textures {
        let bytes = read_entry(&mut archive, index)?;
        let mcmeta = match index_by_name.get(&format!("{name}.mcmeta")) {
            Some(&mcmeta_index) => Some(read_entry(&mut archive, mcmeta_index)?),
            None => None,
        };
        jobs.push(TextureJob {
            name,
            bytes,
            mcmeta,
        });
    }

    let total = jobs.len();""",
    """/// Scale every texture of a jar, a zip or a resource pack directory.
pub fn upscale_source(
    source_path: &Path,
    out_dir: &Path,
    opts: &BatchOptions,
    progress: impl Fn(BatchProgress) + Sync,
) -> Result<BatchReport, String> {
    opts.upscale_config(false).validate()?;

    let source = if source_path.is_dir() {
        read_directory(source_path)?
    } else {
        read_archive(source_path)?
    };

    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("cannot create {}: {error}", out_dir.display()))?;

    let models = if opts.stitch {
        let model_dir = opts.model_dir.clone().or_else(default_model_dir);
        model_dir
            .map(|dir| load_entity_models(&dir))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let block_models = if opts.stitch {
        load_block_models(&source.block_model_entries)
    } else {
        HashMap::new()
    };
    let pack_format = source.pack_format;

    let mut jobs = source.jobs;
    if let Some(only) = opts.only.as_deref() {
        jobs.retain(|job| job.name.contains(only));
    }
    if jobs.is_empty() {
        return Err(format!(
            "no PNG textures found under {TEXTURE_ROOT} in {}",
            source_path.display()
        ));
    }

    let total = jobs.len();""",
)

# The source reading.
swap(
    """fn read_entry(archive: &mut ZipArchive<File>, index: usize) -> Result<Vec<u8>, String> {""",
    """/// The textures and the models of a source.
struct Source {
    jobs: Vec<TextureJob>,
    /// The block models of the source, by their short name.
    block_model_entries: Vec<(String, Vec<u8>)>,
    pack_format: Option<i64>,
}

/// Read the textures and the models out of a jar or a zip.
fn read_archive(path: &Path) -> Result<Source, String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("cannot read jar: {error}"))?;

    let mut index_by_name: HashMap<String, usize> = HashMap::with_capacity(archive.len());
    let mut textures: Vec<(usize, String)> = Vec::new();
    let mut block_model_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let name = archive
            .by_index(index)
            .map_err(|error| format!("cannot list jar entry: {error}"))?
            .name()
            .to_string();
        index_by_name.insert(name.clone(), index);
        if name.starts_with(TEXTURE_ROOT) && name.ends_with(".png") {
            textures.push((index, name));
        } else if name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json") {
            let short = name
                .trim_end_matches(".json")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if !short.is_empty() {
                let bytes = read_entry(&mut archive, index)?;
                block_model_entries.push((short, bytes));
            }
        }
    }
    textures.sort_by(|a, b| a.1.cmp(&b.1));

    // Load the bytes up front so the workers never share the archive.
    let mut jobs = Vec::with_capacity(textures.len());
    for (index, name) in textures {
        let bytes = read_entry(&mut archive, index)?;
        let mcmeta = match index_by_name.get(&format!("{name}.mcmeta")) {
            Some(&mcmeta_index) => Some(read_entry(&mut archive, mcmeta_index)?),
            None => None,
        };
        jobs.push(TextureJob {
            name,
            bytes,
            mcmeta,
        });
    }

    let pack_format = read_pack_format(&mut archive);
    Ok(Source {
        jobs,
        block_model_entries,
        pack_format,
    })
}

/// Read the textures and the models out of a resource pack directory.
fn read_directory(dir: &Path) -> Result<Source, String> {
    let mut jobs = Vec::new();
    let mut block_model_entries = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(dir) else {
                continue;
            };
            let name = relative.to_string_lossy().replace('\\\\', "/");
            if name.starts_with(TEXTURE_ROOT) && name.ends_with(".png") {
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                let mcmeta_path = path.with_extension("png.mcmeta");
                let mcmeta = std::fs::read(&mcmeta_path).ok();
                jobs.push(TextureJob {
                    name,
                    bytes,
                    mcmeta,
                });
            } else if name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json") {
                let short = name
                    .trim_end_matches(".json")
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                if short.is_empty() {
                    continue;
                }
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                block_model_entries.push((short, bytes));
            }
        }
    }

    jobs.sort_by(|a, b| a.name.cmp(&b.name));
    block_model_entries.sort_by(|a, b| a.0.cmp(&b.0));

    // The pack format of the source, when it has one.
    let pack_format = std::fs::read_to_string(dir.join("pack.mcmeta"))
        .ok()
        .and_then(|data| serde_json::from_str::<Value>(&data).ok())
        .and_then(|value| {
            value
                .get("pack")
                .and_then(|pack| pack.get("pack_format"))
                .and_then(Value::as_i64)
        });

    Ok(Source {
        jobs,
        block_model_entries,
        pack_format,
    })
}

fn read_entry(archive: &mut ZipArchive<File>, index: usize) -> Result<Vec<u8>, String> {""",
)

# The block models now come from a list of entries.
swap(
    """fn load_block_models(
    archive: &mut ZipArchive<File>,
    index_by_name: &HashMap<String, usize>,
) -> HashMap<String, ModelFaces> {
    let mut raw: HashMap<String, Value> = HashMap::new();
    let mut names: Vec<&String> = index_by_name
        .keys()
        .filter(|name| name.starts_with(BLOCK_MODEL_ROOT) && name.ends_with(".json"))
        .collect();
    names.sort();
    for name in names {
        let Some(&index) = index_by_name.get(name) else {
            continue;
        };
        let Ok(bytes) = read_entry(archive, index) else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let short = name
            .trim_end_matches(".json")
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        if !short.is_empty() {
            raw.insert(short, value);
        }
    }
""",
    """fn load_block_models(entries: &[(String, Vec<u8>)]) -> HashMap<String, ModelFaces> {
    let mut raw: HashMap<String, Value> = HashMap::new();
    for (short, bytes) in entries {
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        if !short.is_empty() {
            raw.insert(short.clone(), value);
        }
    }
""",
)

# The old entry point stays, it now calls the new one.
swap(
    """/// Scale every texture of a jar, a zip or a resource pack directory.
pub fn upscale_source(""",
    """/// Scale every texture of a jar or a zip.
pub fn upscale_jar(
    jar_path: &Path,
    out_dir: &Path,
    opts: &BatchOptions,
    progress: impl Fn(BatchProgress) + Sync,
) -> Result<BatchReport, String> {
    upscale_source(jar_path, out_dir, opts, progress)
}

/// Scale every texture of a jar, a zip or a resource pack directory.
pub fn upscale_source(""",
)

open(p, "w").write(s)
print("jar.rs reads directories")
