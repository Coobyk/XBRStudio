//! The entity models of the game and the way a texture is matched to one.

use std::path::Path;

use serde_json::Value;

use super::collect_json_files;
use crate::model::{ModelFaces, parse_model};

pub struct EntityModel {
    pub faces: ModelFaces,
    pub has_texture_size: bool,
    /// Normalized class stem, e.g. `adultwolf`.
    pub key: String,
    /// `key` with one leading adult/baby/cold/warm token removed.
    pub base_key: String,
    /// Normalized package directories (every FQCN component but the class).
    pub pkg_dirs: Vec<String>,
    /// Lowercased method name, e.g. `createbodylayer`.
    pub method: String,
    /// Raw model JSON, kept for the 3D preview tool.
    pub raw: Value,
    /// Model file path relative to the model directory (posix separators).
    pub source: String,
}
pub fn normalize_key(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}
pub fn model_key(fqcn: &str) -> String {
    let simple = fqcn.rsplit('.').next().unwrap_or(fqcn);
    let stem = simple.strip_suffix("Model").unwrap_or(simple);
    normalize_key(stem)
}
pub fn entity_family_key(relative_path: &str) -> Option<String> {
    let rest = relative_path.strip_prefix("entity/")?;
    let family = rest.split('/').next()?;
    let key = normalize_key(family);
    if key.is_empty() { None } else { Some(key) }
}

/// Scoring inputs derived from the texture's path inside `textures/`.
#[derive(Debug, Clone)]
pub struct TextureCtx {
    pub family: String,
    /// Second path segment (equipment armor slot), empty when absent.
    pub subdir: String,
    /// Normalized filename stem, e.g. `chickencoldbaby`.
    pub stem: String,
    /// `stem` with trailing baby/cold/warm/temperate/pup tokens removed.
    pub stripped: String,
    pub wants_baby: bool,
    pub wants_cold: bool,
    pub wants_warm: bool,
    pub is_equipment: bool,
}
pub fn strip_key_prefix(key: &str) -> String {
    for prefix in ["adult", "baby", "cold", "warm"] {
        if key.starts_with(prefix) && key.len() > prefix.len() {
            return key[prefix.len()..].to_string();
        }
    }
    key.to_string()
}
pub fn strip_variant_tokens(stem: &str) -> (String, bool, bool, bool) {
    let mut text = stem.to_string();
    let mut wants_baby = false;
    let mut wants_cold = false;
    let mut wants_warm = false;
    while let Some(token) = ["baby", "pup", "cold", "warm", "temperate"]
        .into_iter()
        .find(|token| text.len() > token.len() && text.ends_with(token))
    {
        match token {
            "baby" | "pup" => wants_baby = true,
            "cold" => wants_cold = true,
            "warm" => wants_warm = true,
            _ => {}
        }
        text.truncate(text.len() - token.len());
    }
    (text, wants_baby, wants_cold, wants_warm)
}
pub fn texture_ctx(relative_path: &str) -> Option<TextureCtx> {
    let family = entity_family_key(relative_path)?;
    let rest = relative_path.strip_prefix("entity/")?;
    // Slot subdir only exists for deeper paths like equipment/humanoid/x.png.
    let subdir = if rest.split('/').count() >= 3 {
        rest.split('/')
            .nth(1)
            .map(normalize_key)
            .unwrap_or_default()
    } else {
        String::new()
    };
    let file = rest.rsplit('/').next().unwrap_or(rest);
    let stem = normalize_key(file.strip_suffix(".png").unwrap_or(file));
    if stem.is_empty() {
        return None;
    }
    let is_equipment = family == "equipment";
    // `entity/zombie_villager/baby/plains.png` — age lives in the path, not the stem.
    let path_baby = rest.split('/').any(|segment| segment == "baby");
    let (stripped, stem_baby, wants_cold, wants_warm) = strip_variant_tokens(&stem);
    Some(TextureCtx {
        family,
        subdir,
        stem,
        stripped,
        wants_baby: stem_baby || path_baby,
        wants_cold,
        wants_warm,
        is_equipment,
    })
}
pub fn alias_targets(family: &str) -> &'static [&'static str] {
    match family {
        "cat" => &["feline", "adultfeline", "babyfeline"],
        "leadknot" => &["leashknot"],
        "bear" => &["polarbear"],
        "wither" => &["witherboss"],
        "chestboat" => &["boat"],
        "zombie" => &["humanoid"],
        "horse" => &["abstractequine"],
        _ => &[],
    }
}

/// Maps an equipment subdir (already normalized, so `humanoid_baby` arrives as
/// `humanoidbaby`) onto the model key of the layer that should be stitched.
pub fn equipment_target(subdir: &str) -> Option<&'static str> {
    match subdir {
        "humanoid" | "humanoidbaby" | "humanoidleggings" => Some("humanoid"),
        "wings" => Some("elytra"),
        "llamabody" => Some("llama"),
        "wolfbody" => Some("wolf"),
        "happyghastbody" => Some("happyghastharness"),
        "nautilusbody" => Some("nautilus"),
        "horsebody" => Some("abstractequine"),
        "camelsaddle" | "camelhusksaddle" => Some("camelsaddle"),
        "nautilussaddle" => Some("nautilussaddle"),
        "horsesaddle"
        | "mulesaddle"
        | "donkeysaddle"
        | "skeletonhorsesaddle"
        | "zombiahorsesaddle" => Some("equinesaddle"),
        _ => None,
    }
}

/// Significant-match threshold: any real signal scores ≥ ~2000; a model with
/// no signal at all must not win on face count alone.
const MIN_SIGNAL: i64 = 1500;

/// Armor-layer methods/models. `ends_with("armor")` so `armorstand` (the
/// stand itself) is not mistaken for an armor layer, while `armorstandarmor`
/// and `nautilusarmor` are.
pub fn is_armor_method(method: &str, key: &str) -> bool {
    method.contains("armor") || key.ends_with("armor")
}
pub fn signal_score(model: &EntityModel, ctx: &TextureCtx) -> i64 {
    let key = model.key.as_str();
    let base = model.base_key.as_str();
    let method = model.method.as_str();

    let model_baby = key.starts_with("baby") || method.contains("baby");
    let model_adult = key.starts_with("adult") || method.contains("adult");
    // Baby models may only use stem/base signals when the texture wants a baby.
    let aligned = !(model_baby && !ctx.wants_baby);

    let mut signal = 0i64;

    if key == ctx.family {
        signal += 3000;
    } else if base == ctx.family && aligned {
        // Prefix-keyed models (coldcow, babywolf) earn the family credit here
        // instead of the stronger exact-key match.
        signal += 2600;
    }
    if model.pkg_dirs.iter().any(|dir| dir == &ctx.family) {
        signal += 2200;
    }
    if aligned {
        if ctx.stripped == key {
            signal += 3400;
        } else if ctx.stripped == base {
            signal += 3200;
        } else if (key.starts_with(ctx.stripped.as_str()) && key.len() > ctx.stripped.len())
            || (ctx.stripped.starts_with(key) && ctx.stripped.len() > key.len())
        {
            signal += 3000;
        }
    }
    // Age mismatch: baby texture must not lean on a non-baby model (and vice
    // versa) via alias/stem bonuses that ignore the baby/adult distinction.
    let age_mismatch = (ctx.wants_baby && !model_baby) || (!ctx.wants_baby && model_baby);
    if !age_mismatch && alias_targets(&ctx.family).contains(&key) {
        signal += 2500;
    }

    if ctx.is_equipment {
        if let Some(target) = equipment_target(&ctx.subdir)
            && (key == target || key.starts_with(target))
        {
            signal += 2800;
        }
        if is_armor_method(method, key) {
            signal += 2000;
        }
        if ctx.subdir == "humanoidbaby" && model_baby {
            signal += 1000;
        }
    } else if is_armor_method(method, key) {
        signal -= 10_000;
    }

    let method_chestboat = method.contains("chestboat") || method.contains("chestraft");
    if ctx.family == "chestboat" {
        if method_chestboat {
            signal += 1500;
        }
    } else if method_chestboat {
        signal -= 1500;
    }
    // Vanilla bamboo boats/chest boats use the raft mesh, not the boat mesh.
    if (ctx.family == "boat" || ctx.family == "chestboat") && ctx.stripped == "bamboo" {
        if key == "raft" {
            signal += 4000;
        } else if key == "boat" {
            signal -= 4000;
        }
    }
    // Layer methods win their own texture token: breeze_eyes.png →
    // createEyesLayer, breeze_wind.png → createWindLayer, creaking_eyes → …
    if ctx.family == key
        && !ctx.stripped.is_empty()
        && let Some(rest) = ctx.stripped.strip_prefix(key)
        && !rest.is_empty()
        && method.contains(rest)
    {
        signal += 900;
    }
    if method.contains("pose") {
        signal -= 500;
    }

    if ctx.family == "chest" {
        if ctx.stem.contains("left") {
            if method.contains("left") {
                signal += 1200;
            }
        } else if ctx.stem.contains("right") {
            if method.contains("right") {
                signal += 1200;
            }
        } else if method.contains("single") {
            signal += 1200;
        }
    }

    if ctx.wants_baby && model_baby {
        signal += 800;
    } else if ctx.wants_baby && model_adult {
        signal -= 400;
    } else if !ctx.wants_baby && model_baby {
        signal -= 600;
    } else if !ctx.wants_baby && model_adult {
        signal += 200;
    }

    let model_cold = key.starts_with("cold") || method.contains("cold");
    let model_warm = key.starts_with("warm") || method.contains("warm");
    if ctx.wants_cold {
        if model_cold {
            signal += 800;
        } else {
            signal -= 800;
        }
    } else if ctx.wants_warm {
        if model_warm {
            signal += 800;
        } else {
            signal -= 800;
        }
    } else if model_cold || model_warm {
        signal -= 300;
    }

    if ctx.stripped.starts_with("tropicala") && key.contains("small") {
        signal += 600;
    }
    if ctx.stripped.starts_with("tropicalb") && key.contains("large") {
        signal += 600;
    }

    signal
}
/// How well the UV space of a model fits the texture.
///
/// A model whose UV space is bigger than the texture is squeezed onto it,
/// which makes the stitching blurry, so such a model is penalised. A model
/// that is smaller than the texture is stretched, which the game does as
/// well, so it is not.
pub fn uv_fit_score(model: &EntityModel, image: (u32, u32)) -> i64 {
    if image.0 == 0 || image.1 == 0 {
        return 0;
    }
    let (uv_width, uv_height) = model.faces.uv_size;
    if uv_width <= 0.0 || uv_height <= 0.0 {
        return 0;
    }
    let over_x = (uv_width - image.0 as f32).max(0.0) / uv_width;
    let over_y = (uv_height - image.1 as f32).max(0.0) / uv_height;
    let over = over_x.max(over_y);
    if over <= 0.0 {
        return 0;
    }
    -((over * 4000.0) as i64)
}

pub fn score_model(model: &EntityModel, ctx: &TextureCtx, image: (u32, u32)) -> i64 {
    if signal_score(model, ctx) < MIN_SIGNAL {
        return i64::MIN / 4; // ineligible, but comparable for max()
    }
    let mut score = signal_score(model, ctx)
        + model.faces.faces.len() as i64
        + if model.has_texture_size { 300 } else { 0 }
        + uv_fit_score(model, image);
    // Prefer a model whose declared texture_size matches the image so a 32×32
    // baby pig is not stitched with the 64×64 adult UV layout (and vice versa).
    if model.has_texture_size && image != (0, 0) {
        let (uv_w, uv_h) = model.faces.uv_size;
        if uv_w as u32 == image.0 && uv_h as u32 == image.1 {
            score += 2000;
        }
    }
    score
}
pub fn best_model_for<'a>(
    models: &'a [EntityModel],
    ctx: &TextureCtx,
    image: (u32, u32),
) -> Option<&'a EntityModel> {
    let strict = models
        .iter()
        .filter(|model| model_eligible(model, ctx))
        .max_by_key(|model| score_model(model, ctx, image));
    // Keep every texture paired: if the strict rules reject everything (odd
    // alias layout), fall back to plain signal-threshold scoring rather than
    // dropping it.
    strict.or_else(|| {
        models
            .iter()
            .filter(|model| signal_score(model, ctx) >= MIN_SIGNAL)
            .max_by_key(|model| score_model(model, ctx, image))
    })
}

/// Pairing eligibility, stricter than the raw signal threshold. A wrong
/// pairing is glaring in 3D (wrong proportions, stray hat/nose cubes) and
/// bakes damaged textures (stitching clears texels the wrong model never
/// covers), so:
///
/// 1. Baby models never serve an adult texture (the signal gate allows it when
///    the package and texture_size bonuses outweigh the age penalty).
/// 2. Families with an explicit alias target (`zombie` → `humanoid`,
///    `horse` → `abstractequine`, …) reject models that are neither that
///    target nor name-equivalent to the texture. This stops
///    `ZombieVillagerModel` from stealing `zombie.png`/`husk.png` via its
///    `monster/zombie` package path and its `texture_size: [64, 64]` match.
pub fn model_eligible(model: &EntityModel, ctx: &TextureCtx) -> bool {
    if signal_score(model, ctx) < MIN_SIGNAL {
        return false;
    }
    let key = model.key.as_str();
    let base = model.base_key.as_str();
    let model_baby = key.starts_with("baby") || model.method.contains("baby");
    // Equipment encodes age in the subdir (`humanoid_baby`) and is already
    // scored precisely by `signal_score`; the age ban only applies to skins.
    if !ctx.is_equipment && model_baby && !ctx.wants_baby {
        return false;
    }
    let aliases = alias_targets(&ctx.family);
    if aliases.is_empty() || aliases.contains(&key) {
        return true;
    }
    // Name-equivalent to the texture: exact keys, prefixes the texture grew
    // around (drowned_outer_layer → drowned), or babies of them.
    key == ctx.family
        || base == ctx.family
        || key == ctx.stripped
        || key == ctx.stem
        || base == ctx.stripped
        || ctx.stripped.starts_with(key)
        || ctx.stripped.starts_with(base)
        || (model_baby && ctx.wants_baby && model.pkg_dirs.iter().any(|dir| dir == &ctx.family))
}
pub fn load_entity_models(model_dir: &Path) -> Vec<EntityModel> {
    let mut json_files = Vec::new();
    collect_json_files(model_dir, &mut json_files);
    json_files.sort();

    let mut models = Vec::new();
    for path in json_files {
        let Ok(data) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if !value.get("parts").is_some_and(|parts| parts.is_array()) {
            continue;
        }
        let Some(fqcn) = value.get("model").and_then(Value::as_str) else {
            continue;
        };
        let key = model_key(fqcn);
        if key.is_empty() {
            continue;
        }
        let has_texture_size = value
            .get("texture_size")
            .is_some_and(|size| !size.is_null());
        let Ok(parsed) = parse_model(&value) else {
            continue;
        };
        if parsed.faces.is_empty() {
            continue;
        }
        let method = value
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let pkg_dirs = fqcn
            .split('.')
            .take(fqcn.split('.').count().saturating_sub(1))
            .map(normalize_key)
            .collect::<Vec<_>>();
        let source = path
            .strip_prefix(model_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        models.push(EntityModel {
            faces: parsed,
            has_texture_size,
            base_key: strip_key_prefix(&key),
            key,
            pkg_dirs,
            method,
            raw: value,
            source,
        });
    }

    models
}
