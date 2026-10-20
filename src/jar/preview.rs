//! The data of the 3D preview: the textures and the models that belong
//! together.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use serde_json::Value;
use zip::ZipArchive;

use super::entities::{best_model_for, load_entity_models, texture_ctx};
use super::{TEXTURE_ROOT, animation_frame_size, collect_png_files, entry_id, read_entry};

/// A texture↔model pairing for the 3D preview tool.
#[derive(Debug, Clone)]
pub struct PreviewPair {
    /// Texture path relative to `textures/` (e.g. `entity/cow/cow_temperate.png`).
    pub texture_rel: String,
    /// Preview id: `texture_rel` without the `.png` suffix.
    pub id: String,
    /// Texture file stem, shown in the picker.
    pub name: String,
    /// Key into [`PreviewData::models`].
    pub model_source: String,
    /// Original texture size in pixels.
    pub orig_size: (u32, u32),
    /// First animation frame in original-texture pixels, when animated.
    pub frame: Option<(u32, u32, u32, u32)>,
}

/// Everything the preview tool needs: pairs, raw model JSONs, original PNGs.
#[derive(Debug)]
pub struct PreviewData {
    pub pairs: Vec<PreviewPair>,
    pub models: HashMap<String, Value>,
    /// Original PNG bytes keyed by [`PreviewPair::id`].
    pub orig_textures: HashMap<String, Vec<u8>>,
}

/// Pair every upscaled entity texture under `textures_root` with its best
/// model from `model_dir`, loading original textures and mcmeta from the jar.
pub fn load_preview_pairs(
    model_dir: &Path,
    textures_root: &Path,
    jar_path: &Path,
) -> Result<PreviewData, String> {
    let entity_models = load_entity_models(model_dir);
    if entity_models.is_empty() {
        return Err(format!(
            "no entity models found under {}",
            model_dir.display()
        ));
    }

    let file = File::open(jar_path)
        .map_err(|error| format!("cannot open {}: {error}", jar_path.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("cannot read jar: {error}"))?;
    let mut index_by_name: HashMap<String, usize> = HashMap::with_capacity(archive.len());
    for index in 0..archive.len() {
        let name = archive
            .by_index(index)
            .map_err(|error| format!("cannot list jar entry: {error}"))?
            .name()
            .to_string();
        index_by_name.insert(name, index);
    }

    let entity_root = textures_root.join("entity");
    let mut pngs = Vec::new();
    collect_png_files(&entity_root, &mut pngs);
    pngs.sort();

    let mut pairs = Vec::new();
    let mut model_jsons: HashMap<String, Value> = HashMap::new();
    let mut orig_textures = HashMap::new();

    for path in pngs {
        let Ok(rel) = path.strip_prefix(textures_root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let Some(ctx) = texture_ctx(&rel) else {
            continue;
        };
        let entry_name = format!("{TEXTURE_ROOT}{rel}");
        let Some(&index) = index_by_name.get(&entry_name) else {
            continue;
        };
        let Ok(orig_bytes) = read_entry(&mut archive, index) else {
            continue;
        };
        let Ok((orig_w, orig_h)) =
            image::load_from_memory(&orig_bytes).map(|image| (image.width(), image.height()))
        else {
            continue;
        };
        if orig_w == 0 || orig_h == 0 {
            continue;
        }
        let Some(model) = best_model_for(&entity_models, &ctx, (orig_w, orig_h)) else {
            continue;
        };

        let frame = index_by_name
            .get(&format!("{entry_name}.mcmeta"))
            .copied()
            .and_then(|mcmeta_index| read_entry(&mut archive, mcmeta_index).ok())
            .and_then(|mcmeta| serde_json::from_slice::<Value>(&mcmeta).ok())
            .and_then(|value| value.get("animation").cloned())
            .filter(|animation| !animation.is_null())
            .map(|animation| {
                let (frame_w, frame_h) = animation_frame_size(orig_w, orig_h, &animation);
                (0, 0, frame_w, frame_h)
            });

        let id = entry_id(&rel);
        let name = id.rsplit('/').next().unwrap_or(&id).to_string();
        model_jsons
            .entry(model.source.clone())
            .or_insert_with(|| model.raw.clone());
        orig_textures.insert(id.clone(), orig_bytes);
        pairs.push(PreviewPair {
            texture_rel: rel,
            id,
            name,
            model_source: model.source.clone(),
            orig_size: (orig_w, orig_h),
            frame,
        });
    }

    if pairs.is_empty() {
        return Err("no upscaled entity textures paired with a model".into());
    }
    Ok(PreviewData {
        pairs,
        models: model_jsons,
        orig_textures,
    })
}
