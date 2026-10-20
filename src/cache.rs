//! Resume cache.
//!
//! With `--resume` the batch skips textures whose source and settings did not
//! change since the last run. The cache is a small JSON file next to the
//! output directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The cache file name inside the output directory.
pub const CACHE_FILE_NAME: &str = ".xbrstudio-cache.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CacheFile {
    #[serde(default)]
    entries: HashMap<String, String>,
}

/// Tracks which textures are already up to date.
#[derive(Debug, Clone)]
pub struct Cache {
    path: PathBuf,
    entries: HashMap<String, String>,
}

impl Cache {
    /// Load the cache from `out_dir`, or start empty when it does not exist.
    pub fn load(out_dir: &Path) -> Cache {
        let path = out_dir.join(CACHE_FILE_NAME);
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<CacheFile>(&data).ok())
            .map(|file| file.entries)
            .unwrap_or_default();
        Cache { path, entries }
    }

    /// True when `key` was processed with exactly these settings.
    pub fn is_fresh(&self, key: &str, fingerprint: &str) -> bool {
        self.entries.get(key).map(String::as_str) == Some(fingerprint)
    }

    /// Remember that `key` was processed with these settings.
    pub fn record(&mut self, key: &str, fingerprint: &str) {
        self.entries
            .insert(key.to_string(), fingerprint.to_string());
    }

    /// Write the cache back to disk.
    pub fn save(&self) -> Result<(), String> {
        let file = CacheFile {
            entries: self.entries.clone(),
        };
        let data =
            serde_json::to_vec_pretty(&file).map_err(|error| format!("encode cache: {error}"))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        std::fs::write(&self.path, data)
            .map_err(|error| format!("cannot write {}: {error}", self.path.display()))?;
        Ok(())
    }

    /// Number of remembered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Fingerprint of a texture: its bytes plus the settings that shape the
/// output.
pub fn fingerprint(bytes: &[u8], settings: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.update(b"\x00");
    hasher.update(settings.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A short description of the settings that influence the output.
pub fn settings_fingerprint(
    factor: u32,
    algorithm: &str,
    alpha: &str,
    sharpen: f32,
    stitch: bool,
    wrap: bool,
) -> String {
    format!(
        "v2|factor={factor}|algorithm={algorithm}|alpha={alpha}|sharpen={sharpen}|stitch={stitch}|wrap={wrap}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_and_forgets_entries() {
        let dir = std::env::temp_dir().join(format!("xbrstudio-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut cache = Cache::load(&dir);
        assert!(cache.is_empty());
        assert!(!cache.is_fresh("stone.png", "abc"));

        cache.record("stone.png", "abc");
        assert!(cache.is_fresh("stone.png", "abc"));
        assert!(!cache.is_fresh("stone.png", "def"));
        cache.save().unwrap();

        let reloaded = Cache::load(&dir);
        assert!(reloaded.is_fresh("stone.png", "abc"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fingerprints_change_with_the_content_and_the_settings() {
        let base = fingerprint(b"texture", "settings");
        assert_eq!(base, fingerprint(b"texture", "settings"));
        assert_ne!(base, fingerprint(b"texture2", "settings"));
        assert_ne!(base, fingerprint(b"texture", "other"));
    }
}
