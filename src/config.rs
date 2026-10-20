//! Optional configuration file.
//!
//! Every setting can live in `xbrstudio.toml` instead of on the command line:
//!
//! ```toml
//! factor = 4
//! algorithm = "xbrz"
//! alpha = "premultiplied"
//! sharpen = 0.0
//! stitch = true
//! wrap = true
//! pack_format = 97
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scalers::{Algorithm, AlphaMode};

/// The file name looked up in the working directory and the user config
/// directory.
pub const CONFIG_FILE_NAME: &str = "xbrstudio.toml";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Default scale factor (2-6).
    pub factor: Option<u32>,
    /// Default upscaling algorithm.
    pub algorithm: Option<Algorithm>,
    /// Default alpha handling.
    pub alpha: Option<AlphaMode>,
    /// Default sharpening amount.
    pub sharpen: Option<f32>,
    /// Stitch model faces by default.
    pub stitch: Option<bool>,
    /// Use the tiled wrap border for block textures.
    pub wrap: Option<bool>,
    /// Directory with entity model JSONs.
    pub models: Option<String>,
    /// `pack_format` written into pack.mcmeta.
    pub pack_format: Option<i64>,
    /// Description written into pack.mcmeta.
    pub pack_description: Option<String>,
    /// OptiFine mipmap levels.
    pub mipmap_levels: Option<u32>,
    /// Generate OptiFine connected textures.
    pub connected_textures: Option<bool>,
    /// Skip textures that are already up to date.
    pub resume: Option<bool>,
    /// Use the GPU backend when it is available.
    pub gpu: Option<bool>,
}

impl Config {
    /// Load the configuration.
    ///
    /// With an explicit path the file must exist. Without one, the working
    /// directory and the user config directory are searched; a missing file
    /// is not an error.
    pub fn load(path: Option<&Path>) -> Result<Config, String> {
        let path = match path {
            Some(path) => {
                if !path.exists() {
                    return Err(format!("config file {} does not exist", path.display()));
                }
                Some(path.to_path_buf())
            }
            None => find_config_file(),
        };

        let Some(path) = path else {
            return Ok(Config::default());
        };

        let data = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let config: Config = toml::from_str(&data)
            .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
        Ok(config)
    }
}

/// Search order: `./xbrstudio.toml`, then the user config directory.
pub fn find_config_file() -> Option<PathBuf> {
    let local = PathBuf::from(CONFIG_FILE_NAME);
    if local.is_file() {
        return Some(local);
    }
    let user = dirs::config_dir()?.join("xbrstudio").join(CONFIG_FILE_NAME);
    if user.is_file() {
        return Some(user);
    }
    None
}

impl Serialize for Algorithm {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Algorithm {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Algorithm::from_name(&name).map_err(serde::de::Error::custom)
    }
}

impl Serialize for AlphaMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for AlphaMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        AlphaMode::from_name(&name).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_config_file() {
        let config: Config = toml::from_str(
            r#"
            factor = 3
            algorithm = "fsr"
            alpha = "masked"
            sharpen = 0.25
            stitch = false
            "#,
        )
        .unwrap();
        assert_eq!(config.factor, Some(3));
        assert_eq!(config.algorithm, Some(Algorithm::Fsr));
        assert_eq!(config.alpha, Some(AlphaMode::Masked));
        assert_eq!(config.sharpen, Some(0.25));
        assert_eq!(config.stitch, Some(false));
    }

    #[test]
    fn rejects_unknown_keys() {
        let error = toml::from_str::<Config>("nonsense = 1").unwrap_err();
        assert!(error.to_string().contains("nonsense"));
    }

    #[test]
    fn rejects_unknown_algorithms() {
        let error = toml::from_str::<Config>(r#"algorithm = "superres""#).unwrap_err();
        assert!(error.to_string().contains("superres"));
    }
}
