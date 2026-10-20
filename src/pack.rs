//! Resource pack packaging: `pack.mcmeta`, zipping and installation.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use zip::write::{FileOptions, SimpleFileOptions, ZipWriter};
use zip::{CompressionMethod, ZipArchive};

/// Everything that ends up in `pack.mcmeta`.
#[derive(Debug, Clone, Default)]
pub struct PackOptions {
    /// Overrides the `pack_format` read from the source jar.
    pub pack_format: Option<i64>,
    /// Overrides the description.
    pub description: Option<String>,
    /// OptiFine mipmap levels (`mipmap_levels`).
    pub mipmap_levels: Option<u32>,
    /// `supported_formats.min_inclusive`.
    pub min_format: Option<i64>,
    /// `supported_formats.max_inclusive`.
    pub max_format: Option<i64>,
}

/// Write `pack.mcmeta` into `out_dir`.
///
/// An existing file is kept unless it is overwritten on purpose, so a pack
/// directory can be assembled from several runs.
pub fn write_pack_mcmeta(
    out_dir: &Path,
    pack_format: Option<i64>,
    factor: u32,
    opts: &PackOptions,
) -> Result<(), String> {
    let path = out_dir.join("pack.mcmeta");
    if path.exists() {
        return Ok(());
    }

    let description = opts
        .description
        .clone()
        .unwrap_or_else(|| format!("Upscaled x{factor} by XBR Studio"));

    let mut pack = serde_json::Map::new();
    pack.insert(
        "pack_format".to_string(),
        json!(opts.pack_format.or(pack_format).unwrap_or(1)),
    );
    pack.insert("description".to_string(), json!(description));

    if opts.min_format.is_some() || opts.max_format.is_some() {
        let mut supported = serde_json::Map::new();
        if let Some(min) = opts.min_format {
            supported.insert("min_inclusive".to_string(), json!(min));
        }
        if let Some(max) = opts.max_format {
            supported.insert("max_inclusive".to_string(), json!(max));
        }
        pack.insert("supported_formats".to_string(), Value::Object(supported));
    }

    if let Some(levels) = opts.mipmap_levels {
        pack.insert("mipmap_levels".to_string(), json!(levels));
    }

    let value = json!({ "pack": Value::Object(pack) });
    let encoded = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("encode pack.mcmeta: {error}"))?;
    std::fs::write(&path, encoded)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(())
}

/// Zip a resource pack directory.
///
/// The archive mirrors the directory layout, so `pack.mcmeta` ends up at the
/// root and `assets/...` below it — exactly what Minecraft expects.
pub fn zip_directory(dir: &Path, zip_path: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    if let Some(parent) = zip_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }

    let file = File::create(zip_path)
        .map_err(|error| format!("cannot create {}: {error}", zip_path.display()))?;
    let mut writer = ZipWriter::new(file);
    let options: SimpleFileOptions = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut entries: Vec<PathBuf> = Vec::new();
    collect_files(dir, &mut entries)?;
    entries.sort();

    for entry in entries {
        let name = entry
            .strip_prefix(dir)
            .map_err(|_| "path escaped the pack directory".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        writer
            .start_file(name, options)
            .map_err(|error| format!("cannot add {}: {error}", entry.display()))?;
        let bytes = std::fs::read(&entry)
            .map_err(|error| format!("cannot read {}: {error}", entry.display()))?;
        writer
            .write_all(&bytes)
            .map_err(|error| format!("cannot write {}: {error}", entry.display()))?;
    }

    writer
        .finish()
        .map_err(|error| format!("cannot finish {}: {error}", zip_path.display()))?;
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// The resource pack folders of the Minecraft installations found on this
/// machine.
pub fn minecraft_resource_pack_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let mut candidates = vec![home.join(".minecraft")];
        if let Some(data) = dirs::data_dir() {
            candidates.push(data.join("minecraft"));
            candidates.push(data.join(".minecraft"));
            candidates.push(data.join("PrismLauncher").join("instances"));
            candidates.push(data.join("compatibility-profiles").join("promise"));
            candidates.push(data.join("multimc"));
            candidates.push(data.join("Minecraft"));
            candidates.push(data.join("minecraft-runtime"));
        }
        for candidate in candidates {
            let packs = candidate.join("resourcepacks");
            if packs.is_dir() && !dirs.iter().any(|dir| dir == &packs) {
                dirs.push(packs);
            }
        }
    }

    dirs
}

/// Copy a pack zip into a Minecraft resource pack folder.
///
/// The first existing installation wins; when none is found the error lists
/// the folders that were looked at.
pub fn install_pack(zip_path: &Path) -> Result<PathBuf, String> {
    if !zip_path.is_file() {
        return Err(format!("{} is not a file", zip_path.display()));
    }
    let targets = minecraft_resource_pack_dirs();
    let Some(target) = targets.first() else {
        return Err(format!(
            "no Minecraft resource pack folder found (looked for {})",
            minecraft_resource_pack_dirs()
                .iter()
                .map(|dir| dir.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };

    let file_name = zip_path
        .file_name()
        .ok_or_else(|| "the pack zip has no file name".to_string())?;
    let destination = target.join(file_name);
    std::fs::copy(zip_path, &destination)
        .map_err(|error| format!("cannot install into {}: {error}", destination.display()))?;
    Ok(destination)
}

/// Read `pack.mcmeta` from a zip archive.
pub fn read_pack_mcmeta(zip_path: &Path) -> Option<Value> {
    let file = File::open(zip_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name("pack.mcmeta").ok()?;
    let mut data = String::new();
    entry.read_to_string(&mut data).ok()?;
    serde_json::from_str(&data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("xbrstudio-{name}-{}", std::process::id()))
    }

    #[test]
    fn writes_pack_mcmeta_with_the_pack_format() {
        let dir = temp_dir("pack-mcmeta");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        write_pack_mcmeta(&dir, Some(97), 4, &PackOptions::default()).unwrap();
        let data = std::fs::read_to_string(dir.join("pack.mcmeta")).unwrap();
        let value: Value = serde_json::from_str(&data).unwrap();
        assert_eq!(value["pack"]["pack_format"], json!(97));
        assert!(
            value["pack"]["description"]
                .as_str()
                .unwrap()
                .contains("x4")
        );

        // An existing file is not overwritten.
        write_pack_mcmeta(&dir, Some(1), 2, &PackOptions::default()).unwrap();
        let data = std::fs::read_to_string(dir.join("pack.mcmeta")).unwrap();
        assert!(data.contains("97"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_mipmap_levels_and_supported_formats() {
        let dir = temp_dir("pack-mipmap");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let opts = PackOptions {
            mipmap_levels: Some(4),
            min_format: Some(10),
            max_format: Some(99),
            ..PackOptions::default()
        };
        write_pack_mcmeta(&dir, Some(97), 4, &opts).unwrap();
        let data = std::fs::read_to_string(dir.join("pack.mcmeta")).unwrap();
        let value: Value = serde_json::from_str(&data).unwrap();
        assert_eq!(value["pack"]["mipmap_levels"], json!(4));
        assert_eq!(
            value["pack"]["supported_formats"]["min_inclusive"],
            json!(10)
        );
        assert_eq!(
            value["pack"]["supported_formats"]["max_inclusive"],
            json!(99)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zips_a_directory() {
        let dir = temp_dir("pack-zip");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets/minecraft/textures/block")).unwrap();
        std::fs::write(dir.join("pack.mcmeta"), "{}").unwrap();
        std::fs::write(dir.join("assets/minecraft/textures/block/stone.png"), "x").unwrap();

        let zip_path = dir.join("out.zip");
        zip_directory(&dir, &zip_path).unwrap();

        let file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert!(archive.by_name("pack.mcmeta").is_ok());
        assert!(
            archive
                .by_name("assets/minecraft/textures/block/stone.png")
                .is_ok()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
