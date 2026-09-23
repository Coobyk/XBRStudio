use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use image::{DynamicImage, RgbaImage};
use serde_json::{Value, json};
use zip::ZipArchive;

use crate::model::{ModelFaces, parse_model, scale_model_faces_to_image};
use crate::upscaling::{UpscaleConfig, upscale_image, upscale_wrapped};

pub const TEXTURE_ROOT: &str = "assets/minecraft/textures/";

#[derive(Debug, Clone)]
pub struct BatchReport {
    pub total: usize,
    pub upscaled: usize,
    pub stitched: usize,
    pub animated: usize,
    pub wrapped: usize,
    pub errors: Vec<(String, String)>,
    pub entity_models: usize,
    /// Non-animated `entity/` textures that found no model (or lost too many
    /// faces to the clip gate), sorted by path.
    pub unmatched_entity: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum BatchProgress {
    Started {
        total: usize,
        entity_models: usize,
    },
    Texture {
        done: usize,
        total: usize,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub enum JarMessage {
    Progress(BatchProgress),
    Finished(Result<BatchReport, String>),
}

#[derive(Debug, Clone)]
pub struct BatchOptions {
    pub factor: u32,
    pub stitch: bool,
    /// `None` auto-discovers `models/entity`; `Some(path)` uses that directory.
    pub model_dir: Option<PathBuf>,
    /// Upscale `textures/block/**` and the beacon beam with a 1px self-tiling
    /// border so edges get wrap context (border scaled away afterwards).
    pub wrap: bool,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            factor: 4,
            stitch: true,
            model_dir: None,
            wrap: true,
        }
    }
}

/// True when `path` contains a `block` directory component, e.g.
/// `assets/minecraft/textures/block/stone.png`.
pub fn is_block_texture_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str().to_str() == Some("block"))
}

/// Relative (to `textures/`) paths that get a 1px self-tiling wrap border:
/// `block/**` plus the beacon beam, which tiles around the beam column.
pub fn wants_wrap(relative: &str) -> bool {
    relative.starts_with("block/") || relative == "entity/beacon/beacon_beam.png"
}

/// Filesystem-path form of [`wants_wrap`] for single-texture mode.
pub fn is_wrap_texture_path(path: &Path) -> bool {
    if is_block_texture_path(path) {
        return true;
    }
    path.file_name()
        .is_some_and(|name| name == "beacon_beam.png")
}

/// Grass/foliage colormaps (`textures/colormap/**`) — biome lookup tables,
/// not something to upscale.
pub fn is_colormap_texture_path(path: &str) -> bool {
    path.starts_with("colormap/") || path.contains("/textures/colormap/")
}

struct EntityModel {
    faces: ModelFaces,
    has_texture_size: bool,
    /// Normalized class stem, e.g. `adultwolf`.
    key: String,
    /// `key` with one leading adult/baby/cold/warm token removed.
    base_key: String,
    /// Normalized package directories (every FQCN component but the class).
    pkg_dirs: Vec<String>,
    /// Lowercased method name, e.g. `createbodylayer`.
    method: String,
}

struct ProcessOutcome {
    animated: bool,
    stitched: bool,
    wrapped: bool,
}

pub fn default_model_dir() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("models/entity")];
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join("models/entity"));
        }
    }
    candidates.into_iter().find(|path| path.is_dir())
}

pub fn spawn_batch(
    jar_path: PathBuf,
    out_dir: PathBuf,
    opts: BatchOptions,
) -> std::sync::mpsc::Receiver<JarMessage> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = upscale_jar(&jar_path, &out_dir, &opts, |message| {
            let _ = sender.send(JarMessage::Progress(message));
        });
        let _ = sender.send(JarMessage::Finished(result));
    });
    receiver
}

pub fn upscale_jar(
    jar_path: &Path,
    out_dir: &Path,
    opts: &BatchOptions,
    mut progress: impl FnMut(BatchProgress),
) -> Result<BatchReport, String> {
    UpscaleConfig {
        factor: opts.factor,
        stitch_faces: false,
    }
    .validate()?;

    let file = File::open(jar_path).map_err(|error| {
        format!("cannot open {}: {error}", jar_path.display())
    })?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("cannot read jar: {error}"))?;

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
            let short = name
                .strip_prefix(TEXTURE_ROOT)
                .map(str::to_string)
                .unwrap_or_else(|| name.clone());
            if is_colormap_texture_path(&short) {
                continue;
            }
            textures.push((index, name));
        }
    }
    textures.sort_by(|a, b| a.1.cmp(&b.1));
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
    let pack_format = read_pack_format(&mut archive);

    let total = textures.len();
    progress(BatchProgress::Started {
        total,
        entity_models: models.len(),
    });

    let mut report = BatchReport {
        total,
        upscaled: 0,
        stitched: 0,
        animated: 0,
        wrapped: 0,
        errors: Vec::new(),
        entity_models: models.len(),
        unmatched_entity: Vec::new(),
    };

    for (position, (index, name)) in textures.iter().enumerate() {
        let short = name.strip_prefix(TEXTURE_ROOT).unwrap_or(name);
        progress(BatchProgress::Texture {
            done: position + 1,
            total,
            name: short.to_string(),
        });

        match process_one(
            &mut archive,
            *index,
            name,
            &index_by_name,
            out_dir,
            opts,
            &models,
        ) {
            Ok(outcome) => {
                report.upscaled += 1;
                if outcome.animated {
                    report.animated += 1;
                }
                if outcome.stitched {
                    report.stitched += 1;
                } else if opts.stitch && !outcome.animated && short.starts_with("entity/") {
                    report.unmatched_entity.push(short.to_string());
                }
                if outcome.wrapped {
                    report.wrapped += 1;
                }
            }
            Err(error) => report.errors.push((short.to_string(), error)),
        }
    }

    if let Err(error) = write_pack_mcmeta(out_dir, pack_format, opts.factor) {
        report.errors.push(("pack.mcmeta".into(), error));
    }

    report.unmatched_entity.sort();
    Ok(report)
}

fn read_entry(archive: &mut ZipArchive<File>, index: usize) -> Result<Vec<u8>, String> {
    let mut entry = archive
        .by_index(index)
        .map_err(|error| format!("cannot open zip entry: {error}"))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read zip entry: {error}"))?;
    Ok(bytes)
}

fn process_one(
    archive: &mut ZipArchive<File>,
    index: usize,
    name: &str,
    index_by_name: &HashMap<String, usize>,
    out_dir: &Path,
    opts: &BatchOptions,
    models: &[EntityModel],
) -> Result<ProcessOutcome, String> {
    let bytes = read_entry(archive, index)?;
    let image = image::load_from_memory(&bytes)
        .map_err(|error| format!("decode PNG: {error}"))?
        .to_rgba8();
    if image.width() == 0 || image.height() == 0 {
        return Err("zero-size texture".into());
    }

    let mcmeta_bytes = match index_by_name.get(&format!("{name}.mcmeta")) {
        Some(&mcmeta_index) => Some(read_entry(archive, mcmeta_index)?),
        None => None,
    };
    let mcmeta_value: Option<Value> = mcmeta_bytes
        .as_ref()
        .and_then(|bytes| serde_json::from_slice(bytes).ok());
    let is_animated = mcmeta_value
        .as_ref()
        .is_some_and(|value| value.get("animation").is_some_and(|item| !item.is_null()));

    let relative = name.strip_prefix(TEXTURE_ROOT).unwrap_or(name);
    let wrap = opts.wrap && wants_wrap(relative);

    let (output, out_mcmeta, animated, stitched) = if is_animated {
        let value = mcmeta_value.clone().expect("checked above");
        let animation = value.get("animation").expect("checked above");
        let upscaled = upscale_animated(&image, animation, opts.factor, wrap)?;
        let mut scaled_value = value;
        scale_animation_fields(&mut scaled_value, opts.factor);
        let encoded = serde_json::to_vec(&scaled_value)
            .map_err(|error| format!("encode mcmeta: {error}"))?;
        (upscaled, Some(encoded), true, false)
    } else {
        let mut local_model: Option<ModelFaces> = None;
        if opts.stitch {
            if let Some(ctx) = texture_ctx(relative) {
                if let Some(model) = best_model_for(models, &ctx) {
                    let original_faces = model.faces.faces.len();
                    let mut model_faces = model.faces.clone();
                    if !model.has_texture_size {
                        model_faces.uv_size = (
                            image.width() as f32,
                            image.height() as f32,
                        );
                    }
                    scale_model_faces_to_image(&mut model_faces, &image);
                    if !model_faces.faces.is_empty()
                        && model_faces.faces.len() * 2 >= original_faces
                    {
                        local_model = Some(model_faces);
                    }
                }
            }
        }
        let stitched = local_model.is_some();
        let upscaled = if stitched {
            upscale_image(
                &image,
                local_model.as_ref().map(|model| model.faces.as_slice()),
                &UpscaleConfig {
                    factor: opts.factor,
                    stitch_faces: true,
                },
            )?
        } else if wrap {
            upscale_wrapped(&image, opts.factor)?
        } else {
            upscale_image(
                &image,
                None,
                &UpscaleConfig {
                    factor: opts.factor,
                    stitch_faces: false,
                },
            )?
        };
        (upscaled, mcmeta_bytes, false, stitched)
    };

    let out_path = out_dir.join(Path::new(name));
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }

    let mut png = Vec::new();
    DynamicImage::ImageRgba8(output)
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| format!("encode PNG: {error}"))?;
    std::fs::write(&out_path, png)
        .map_err(|error| format!("cannot write {}: {error}", out_path.display()))?;

    if let Some(mcmeta) = out_mcmeta {
        let mut mcmeta_path = out_path.clone().into_os_string();
        mcmeta_path.push(".mcmeta");
        let mcmeta_path = PathBuf::from(mcmeta_path);
        std::fs::write(&mcmeta_path, mcmeta)
            .map_err(|error| format!("cannot write {}: {error}", mcmeta_path.display()))?;
    }

    Ok(ProcessOutcome {
        animated,
        stitched,
        wrapped: wrap,
    })
}

fn upscale_animated(
    image: &RgbaImage,
    animation: &Value,
    factor: u32,
    wrap: bool,
) -> Result<RgbaImage, String> {
    let (_frame_width, frame_height) =
        animation_frame_size(image.width(), image.height(), animation);
    let frame_count = (image.height() / frame_height).max(1);

    let mut bands: Vec<RgbaImage> = Vec::with_capacity(frame_count as usize);
    for frame in 0..frame_count {
        let y = frame * frame_height;
        let band_height = frame_height.min(image.height() - y);
        let band = image::imageops::crop_imm(image, 0, y, image.width(), band_height).to_image();
        let scaled = if wrap {
            upscale_wrapped(&band, factor)?
        } else {
            upscale_image(
                &band,
                None,
                &UpscaleConfig {
                    factor,
                    stitch_faces: false,
                },
            )?
        };
        bands.push(scaled);
    }

    let out_width = image.width() * factor;
    let out_height: u32 = bands.iter().map(RgbaImage::height).sum();
    let mut out = RgbaImage::new(out_width, out_height);
    let mut y = 0;
    for band in bands {
        let height = band.height();
        for row in 0..height {
            for x in 0..band.width() {
                out.put_pixel(x, y + row, *band.get_pixel(x, row));
            }
        }
        y += height;
    }
    Ok(out)
}

fn animation_frame_size(width: u32, height: u32, animation: &Value) -> (u32, u32) {
    let frame_width = animation
        .get("width")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .map(|value| value as u32)
        .unwrap_or(width);
    let frame_height = animation
        .get("height")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .map(|value| value as u32)
        .unwrap_or(frame_width);
    (frame_width, frame_height.max(1).min(height.max(1)))
}

fn scale_animation_fields(value: &mut Value, factor: u32) {
    let Some(animation) = value.get_mut("animation").and_then(Value::as_object_mut) else {
        return;
    };
    for key in ["width", "height"] {
        if let Some(field) = animation.get_mut(key) {
            if let Some(pixels) = field.as_u64() {
                *field = json!(pixels * u64::from(factor));
            }
        }
    }
}

fn normalize_key(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn model_key(fqcn: &str) -> String {
    let simple = fqcn.rsplit('.').next().unwrap_or(fqcn);
    let stem = simple.strip_suffix("Model").unwrap_or(simple);
    normalize_key(stem)
}

fn entity_family_key(relative_path: &str) -> Option<String> {
    let rest = relative_path.strip_prefix("entity/")?;
    let family = rest.split('/').next()?;
    let key = normalize_key(family);
    if key.is_empty() { None } else { Some(key) }
}

/// Scoring inputs derived from the texture's path inside `textures/`.
#[derive(Debug, Clone)]
struct TextureCtx {
    family: String,
    /// Second path segment (equipment armor slot), empty when absent.
    subdir: String,
    /// Normalized filename stem, e.g. `chickencoldbaby`.
    stem: String,
    /// `stem` with trailing baby/cold/warm/temperate/pup tokens removed.
    stripped: String,
    wants_baby: bool,
    wants_cold: bool,
    wants_warm: bool,
    is_equipment: bool,
}

fn strip_key_prefix(key: &str) -> String {
    for prefix in ["adult", "baby", "cold", "warm"] {
        if key.starts_with(prefix) && key.len() > prefix.len() {
            return key[prefix.len()..].to_string();
        }
    }
    key.to_string()
}

fn strip_variant_tokens(stem: &str) -> (String, bool, bool, bool) {
    let mut text = stem.to_string();
    let mut wants_baby = false;
    let mut wants_cold = false;
    let mut wants_warm = false;
    loop {
        let Some(token) = ["baby", "pup", "cold", "warm", "temperate"]
            .into_iter()
            .find(|token| text.len() > token.len() && text.ends_with(token))
        else {
            break;
        };
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

fn texture_ctx(relative_path: &str) -> Option<TextureCtx> {
    let family = entity_family_key(relative_path)?;
    let rest = relative_path.strip_prefix("entity/")?;
    // Slot subdir only exists for deeper paths like equipment/humanoid/x.png.
    let subdir = if rest.split('/').count() >= 3 {
        rest.split('/').nth(1).map(normalize_key).unwrap_or_default()
    } else {
        String::new()
    };
    let file = rest.rsplit('/').next().unwrap_or(rest);
    let stem = normalize_key(file.strip_suffix(".png").unwrap_or(file));
    if stem.is_empty() {
        return None;
    }
    let is_equipment = family == "equipment";
    let (stripped, wants_baby, wants_cold, wants_warm) = strip_variant_tokens(&stem);
    Some(TextureCtx {
        family,
        subdir,
        stem,
        stripped,
        wants_baby,
        wants_cold,
        wants_warm,
        is_equipment,
    })
}

fn alias_targets(family: &str) -> &'static [&'static str] {
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
fn equipment_target(subdir: &str) -> Option<&'static str> {
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
        "horsesaddle" | "mulesaddle" | "donkeysaddle"
        | "skeletonhorsesaddle" | "zombiahorsesaddle" => Some("equinesaddle"),
        _ => None,
    }
}

/// Significant-match threshold: any real signal scores ≥ ~2000; a model with
/// no signal at all must not win on face count alone.
const MIN_SIGNAL: i64 = 1500;

/// Armor-layer methods/models. `ends_with("armor")` so `armorstand` (the
/// stand itself) is not mistaken for an armor layer, while `armorstandarmor`
/// and `nautilusarmor` are.
fn is_armor_method(method: &str, key: &str) -> bool {
    method.contains("armor") || key.ends_with("armor")
}

fn signal_score(model: &EntityModel, ctx: &TextureCtx) -> i64 {
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
        } else if key.starts_with(ctx.stripped.as_str()) && key.len() > ctx.stripped.len() {
            signal += 3000;
        } else if ctx.stripped.starts_with(key) && ctx.stripped.len() > key.len() {
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

    let method_chestboat = method.contains("chestboat");
    if ctx.family == "chestboat" {
        if method_chestboat {
            signal += 1500;
        }
    } else if method_chestboat {
        signal -= 1500;
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
        signal += 600;
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

fn score_model(model: &EntityModel, ctx: &TextureCtx) -> i64 {
    if signal_score(model, ctx) < MIN_SIGNAL {
        return i64::MIN / 4; // ineligible, but comparable for max()
    }
    signal_score(model, ctx) + model.faces.faces.len() as i64
        + if model.has_texture_size { 300 } else { 0 }
}

fn best_model_for<'a>(models: &'a [EntityModel], ctx: &TextureCtx) -> Option<&'a EntityModel> {
    models
        .iter()
        .filter(|model| signal_score(model, ctx) >= MIN_SIGNAL)
        .max_by_key(|model| score_model(model, ctx))
}

fn load_entity_models(model_dir: &Path) -> Vec<EntityModel> {
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
        models.push(EntityModel {
            faces: parsed,
            has_texture_size,
            base_key: strip_key_prefix(&key),
            key,
            pkg_dirs,
            method,
        });
    }

    models
}

fn collect_json_files(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
}

fn read_pack_format(archive: &mut ZipArchive<File>) -> Option<i64> {
    let mut entry = archive.by_name("version.json").ok()?;
    let mut data = String::new();
    entry.read_to_string(&mut data).ok()?;
    let value: Value = serde_json::from_str(&data).ok()?;
    let pack = value.get("pack_version")?;
    if let Some(format) = pack.as_i64() {
        return Some(format);
    }
    pack.get("resource_major").and_then(Value::as_i64)
}

fn write_pack_mcmeta(out_dir: &Path, pack_format: Option<i64>, factor: u32) -> Result<(), String> {
    let path = out_dir.join("pack.mcmeta");
    if path.exists() {
        return Ok(());
    }
    let description = format!("Upscaled x{factor} by XBR Studio");
    let value = match pack_format {
        Some(format) => json!({
            "pack": {
                "pack_format": format,
                "description": description,
            }
        }),
        None => json!({
            "pack": {
                "pack_format": 1,
                "supported_formats": {"min_inclusive": 1, "max_inclusive": 999},
                "description": description,
            }
        }),
    };
    let encoded = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("encode pack.mcmeta: {error}"))?;
    std::fs::write(&path, encoded)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn png_bytes(width: u32, height: u32, fill: [u8; 4]) -> Vec<u8> {
        let image = RgbaImage::from_pixel(width, height, image::Rgba(fill));
        let mut buffer = Vec::new();
        DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
            .unwrap();
        buffer
    }

    fn build_test_jar() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let files: [(&str, Vec<u8>); 10] = [
            (
                "assets/minecraft/textures/block/stone.png",
                png_bytes(4, 4, [200, 50, 50, 255]),
            ),
            (
                "assets/minecraft/textures/block/stone.png.mcmeta",
                br#"{"texture":{"mipmap_strategy":"cutout"}}"#.to_vec(),
            ),
            (
                "assets/minecraft/textures/block/lava_still.png",
                png_bytes(4, 12, [255, 100, 0, 255]),
            ),
            (
                "assets/minecraft/textures/block/lava_still.png.mcmeta",
                br#"{"animation":{"frametime":2,"width":4,"height":4}}"#.to_vec(),
            ),
            (
                "assets/minecraft/textures/entity/cow/test_cow.png",
                png_bytes(16, 16, [120, 80, 40, 255]),
            ),
            (
                "assets/minecraft/textures/entity/wolf/wolf.png",
                png_bytes(32, 16, [160, 160, 160, 255]),
            ),
            (
                "assets/minecraft/textures/entity/equipment/test_armor.png",
                png_bytes(16, 16, [90, 90, 90, 255]),
            ),
            (
                "assets/minecraft/textures/colormap/grass.png",
                png_bytes(256, 256, [10, 200, 10, 255]),
            ),
            (
                "version.json",
                br#"{"pack_version":{"resource_major":97,"resource_minor":1}}"#.to_vec(),
            ),
            ("net/minecraft/Client.class", vec![0u8; 4]),
        ];
        for (name, bytes) in &files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn family_and_model_keys_normalize() {
        assert_eq!(
            entity_family_key("entity/zombie_villager/profession/librarian.png").as_deref(),
            Some("zombievillager")
        );
        assert_eq!(
            entity_family_key("entity/cow/cow_temperate.png").as_deref(),
            Some("cow")
        );
        assert_eq!(entity_family_key("block/stone.png"), None);
        assert_eq!(
            model_key("net.minecraft.client.model.animal.cow.CowModel"),
            "cow"
        );
        assert_eq!(
            model_key("net.minecraft.client.model.ZombieVillagerModel"),
            "zombievillager"
        );
        assert_eq!(model_key("net.minecraft.client.model.PlayerModel"), "player");
        assert_eq!(strip_key_prefix("adultwolf"), "wolf");
        assert_eq!(strip_key_prefix("coldcow"), "cow");
        assert_eq!(strip_key_prefix("cow"), "cow");
        assert_eq!(strip_key_prefix("babyzombievillager"), "zombievillager");
        assert!(texture_ctx("entity/wolf/wolf_baby.png").unwrap().wants_baby);
        assert!(
            texture_ctx("entity/cow/cow_cold.png").unwrap().wants_cold
        );
        assert_eq!(
            texture_ctx("entity/equipment/humanoid/iron.png")
                .unwrap()
                .subdir,
            "humanoid"
        );
        assert_eq!(
            texture_ctx("entity/equipment/test_armor.png")
                .unwrap()
                .subdir,
            ""
        );
    }

    fn test_model(key: &str, method: &str, pkg: &[&str], face_count: usize) -> EntityModel {
        use crate::model::{FaceRect, ModelFace};
        let faces = (0..face_count as u32)
            .map(|index| ModelFace {
                texture: "#0".into(),
                rect: FaceRect {
                    x: index * 2,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                face: "top".into(),
                group: 0,
            })
            .collect();
        EntityModel {
            faces: crate::model::ModelFaces {
                textures: HashMap::new(),
                faces,
                uv_size: (64.0, 64.0),
            },
            has_texture_size: true,
            base_key: strip_key_prefix(key),
            key: key.into(),
            pkg_dirs: pkg.iter().map(|dir| normalize_key(dir)).collect(),
            method: method.into(),
        }
    }

    fn pick<'a>(models: &'a [EntityModel], path: &str) -> Option<&'a EntityModel> {
        let ctx = texture_ctx(path)?;
        best_model_for(models, &ctx)
    }

    fn wolf_models() -> Vec<EntityModel> {
        vec![
            test_model(
                "adultwolf",
                "createbodylayer",
                &["net", "minecraft", "client", "model", "animal", "wolf"],
                60,
            ),
            test_model(
                "babywolf",
                "createbodylayer",
                &["net", "minecraft", "client", "model", "animal", "wolf"],
                54,
            ),
            test_model(
                "cow",
                "createbodylayer",
                &["net", "minecraft", "client", "model", "animal", "cow"],
                50,
            ),
        ]
    }

    #[test]
    fn wolf_textures_prefer_matching_variant() {
        let models = wolf_models();
        assert_eq!(
            pick(&models, "entity/wolf/wolf.png").map(|m| m.key.as_str()),
            Some("adultwolf")
        );
        assert_eq!(
            pick(&models, "entity/wolf/wolf_baby.png").map(|m| m.key.as_str()),
            Some("babywolf")
        );
        assert_eq!(
            pick(&models, "entity/wolf/wolf_angry.png").map(|m| m.key.as_str()),
            Some("adultwolf")
        );
    }

    #[test]
    fn polarbear_baby_prefers_baby_model_over_adult_alias() {
        let models = vec![
            test_model(
                "polarbear",
                "createbodylayer",
                &["net", "m", "model", "animal", "polarbear"],
                60,
            ),
            test_model(
                "babypolarbear",
                "createbodylayer",
                &["net", "m", "model", "animal", "polarbear"],
                54,
            ),
            test_model(
                "wolf",
                "createbodylayer",
                &["net", "m", "model", "animal", "wolf"],
                50,
            ),
        ];
        assert_eq!(
            pick(&models, "entity/bear/polarbear.png").map(|m| m.key.as_str()),
            Some("polarbear")
        );
        assert_eq!(
            pick(&models, "entity/bear/polarbear_baby.png").map(|m| m.key.as_str()),
            Some("babypolarbear")
        );
    }

    #[test]
    fn family_stem_alias_and_variant_matching() {
        let fish = vec![
            test_model(
                "cod",
                "createbodylayer",
                &["net", "m", "model", "animal", "fish"],
                30,
            ),
            test_model(
                "salmon",
                "createbodylayer",
                &["net", "m", "model", "animal", "fish"],
                30,
            ),
        ];
        assert_eq!(
            pick(&fish, "entity/fish/cod.png").map(|m| m.key.as_str()),
            Some("cod")
        );

        let zombies = vec![
            test_model("humanoid", "createmesh", &["net", "m", "model"], 42),
            test_model(
                "drowned",
                "createbodylayer",
                &["net", "m", "model", "monster", "zombie"],
                48,
            ),
            test_model(
                "babyzombie",
                "createbodylayer",
                &["net", "m", "model", "monster", "zombie"],
                40,
            ),
        ];
        assert_eq!(
            pick(&zombies, "entity/zombie/zombie.png").map(|m| m.key.as_str()),
            Some("humanoid")
        );
        assert_eq!(
            pick(&zombies, "entity/zombie/husk.png").map(|m| m.key.as_str()),
            Some("humanoid")
        );
        assert_eq!(
            pick(&zombies, "entity/zombie/drowned.png").map(|m| m.key.as_str()),
            Some("drowned")
        );
        assert_eq!(
            pick(&zombies, "entity/zombie/drowned_baby.png").map(|m| m.key.as_str()),
            Some("drowned")
        );
        assert_eq!(
            pick(&zombies, "entity/zombie/zombie_baby.png").map(|m| m.key.as_str()),
            Some("babyzombie")
        );

        let wither = vec![
            test_model(
                "witherboss",
                "createbodylayer",
                &["net", "m", "model", "monster", "wither"],
                80,
            ),
            test_model(
                "creeper",
                "createbodylayer",
                &["net", "m", "model", "monster", "creeper"],
                40,
            ),
        ];
        assert_eq!(
            pick(&wither, "entity/wither/wither.png").map(|m| m.key.as_str()),
            Some("witherboss")
        );
        let lead = vec![
            test_model(
                "leashknot",
                "createbodylayer",
                &["net", "m", "model", "object", "leash"],
                6,
            ),
            test_model(
                "bell",
                "createbodylayer",
                &["net", "m", "model", "object", "bell"],
                8,
            ),
        ];
        assert_eq!(
            pick(&lead, "entity/lead_knot/lead_knot.png").map(|m| m.key.as_str()),
            Some("leashknot")
        );

        let equines = vec![
            test_model(
                "abstractequine",
                "createbodymesh",
                &["net", "m", "model", "animal", "equine"],
                72,
            ),
            test_model(
                "donkey",
                "createbodylayer",
                &["net", "m", "model", "animal", "equine"],
                72,
            ),
            test_model(
                "babyhorse",
                "createbabymesh",
                &["net", "m", "model", "animal", "equine"],
                60,
            ),
            test_model(
                "babydonkey",
                "createbabymesh",
                &["net", "m", "model", "animal", "equine"],
                60,
            ),
        ];
        assert_eq!(
            pick(&equines, "entity/horse/horse_black.png").map(|m| m.key.as_str()),
            Some("abstractequine")
        );
        assert_eq!(
            pick(&equines, "entity/horse/donkey.png").map(|m| m.key.as_str()),
            Some("donkey")
        );
        assert_eq!(
            pick(&equines, "entity/horse/donkey_baby.png").map(|m| m.key.as_str()),
            Some("babydonkey")
        );

        let cats = vec![
            test_model(
                "adultfeline",
                "createbodymesh",
                &["net", "m", "model", "animal", "feline"],
                48,
            ),
            test_model(
                "babyfeline",
                "createbabylayer",
                &["net", "m", "model", "animal", "feline"],
                40,
            ),
            test_model(
                "cow",
                "createbodylayer",
                &["net", "m", "model", "animal", "cow"],
                50,
            ),
        ];
        assert_eq!(
            pick(&cats, "entity/cat/cat_tabby.png").map(|m| m.key.as_str()),
            Some("adultfeline")
        );
        assert_eq!(
            pick(&cats, "entity/cat/cat_tabby_baby.png").map(|m| m.key.as_str()),
            Some("babyfeline")
        );

        let chests = vec![
            test_model(
                "chest",
                "createsinglebodylayer",
                &["net", "m", "model", "object", "chest"],
                40,
            ),
            test_model(
                "chest",
                "createdoublebodyleftlayer",
                &["net", "m", "model", "object", "chest"],
                50,
            ),
            test_model(
                "chest",
                "createdoublebodyrightlayer",
                &["net", "m", "model", "object", "chest"],
                50,
            ),
        ];
        assert_eq!(
            pick(&chests, "entity/chest/normal.png").map(|m| m.method.as_str()),
            Some("createsinglebodylayer")
        );
        assert_eq!(
            pick(&chests, "entity/chest/normal_left.png").map(|m| m.method.as_str()),
            Some("createdoublebodyleftlayer")
        );
        assert_eq!(
            pick(&chests, "entity/chest/normal_right.png").map(|m| m.method.as_str()),
            Some("createdoublebodyrightlayer")
        );

        let boats = vec![
            test_model(
                "boat",
                "createboatmodel",
                &["net", "m", "model", "object", "boat"],
                60,
            ),
            test_model(
                "boat",
                "createchestboatmodel",
                &["net", "m", "model", "object", "boat"],
                72,
            ),
            test_model(
                "boat",
                "addcommonparts",
                &["net", "m", "model", "object", "boat"],
                48,
            ),
        ];
        assert_eq!(
            pick(&boats, "entity/boat/oak.png").map(|m| m.method.as_str()),
            Some("createboatmodel")
        );
        assert_eq!(
            pick(&boats, "entity/chest_boat/oak.png").map(|m| m.method.as_str()),
            Some("createchestboatmodel")
        );

        let cows = vec![
            test_model(
                "cow",
                "createbodylayer",
                &["net", "m", "model", "animal", "cow"],
                60,
            ),
            test_model(
                "coldcow",
                "createbodylayer",
                &["net", "m", "model", "animal", "cow"],
                60,
            ),
        ];
        assert_eq!(
            pick(&cows, "entity/cow/cow_cold.png").map(|m| m.key.as_str()),
            Some("coldcow")
        );
        assert_eq!(
            pick(&cows, "entity/cow/cow_temperate.png").map(|m| m.key.as_str()),
            Some("cow")
        );
    }

    #[test]
    fn equipment_slots_pick_the_right_layer_model() {
        let models = vec![
            test_model("humanoid", "createmesh", &["net", "m", "model"], 42),
            test_model(
                "humanoid",
                "createbasearmormesh",
                &["net", "m", "model"],
                42,
            ),
            test_model(
                "humanoid",
                "createbabyarmormesh",
                &["net", "m", "model"],
                40,
            ),
            test_model(
                "elytra",
                "createlayer",
                &["net", "m", "model", "object", "equipment"],
                24,
            ),
        ];
        let iron = pick(&models, "entity/equipment/humanoid/iron.png").unwrap();
        assert_eq!(iron.key, "humanoid");
        assert_eq!(iron.method, "createbasearmormesh");

        let baby = pick(&models, "entity/equipment/humanoid_baby/iron.png").unwrap();
        assert_eq!(baby.method, "createbabyarmormesh");

        let wings = pick(&models, "entity/equipment/wings/elytra.png").unwrap();
        assert_eq!(wings.key, "elytra");

        // Saddle subdir with no model at all must not match anything.
        let saddles = vec![test_model(
            "cow",
            "createbodylayer",
            &["net", "m", "model", "animal", "cow"],
            60,
        )];
        assert!(pick(&saddles, "entity/equipment/pig_saddle/saddle.png").is_none());
    }

    #[test]
    fn armor_methods_lose_outside_equipment_and_pose_variants_lose() {
        let models = vec![
            test_model(
                "armorstand",
                "createbodylayer",
                &["net", "m", "model", "object", "armorstand"],
                60,
            ),
            test_model(
                "armorstandarmor",
                "createbodylayer",
                &["net", "m", "model", "object", "armorstand"],
                40,
            ),
            test_model(
                "sniffer",
                "createbodylayer",
                &["net", "m", "model", "animal", "sniffer"],
                70,
            ),
            test_model(
                "sniffer",
                "createsittingposebodylayer",
                &["net", "m", "model", "animal", "sniffer"],
                80,
            ),
        ];
        assert_eq!(
            pick(&models, "entity/armorstand/armorstand.png").map(|m| m.key.as_str()),
            Some("armorstand")
        );
        assert_eq!(
            pick(&models, "entity/sniffer/sniffer.png").map(|m| m.method.as_str()),
            Some("createbodylayer")
        );
    }

    #[test]
    fn animation_frame_size_defaults_to_square_frames() {
        assert_eq!(animation_frame_size(16, 320, &json!({})), (16, 16));
        assert_eq!(
            animation_frame_size(16, 320, &json!({"width": 8})),
            (8, 8)
        );
        assert_eq!(
            animation_frame_size(16, 320, &json!({"width": 4, "height": 2})),
            (4, 2)
        );
    }

    #[test]
    fn is_block_texture_path_detects_block_directory() {
        assert!(is_block_texture_path(Path::new(
            "assets/minecraft/textures/block/stone.png"
        )));
        assert!(is_block_texture_path(Path::new("textures/block/dirt.png")));
        assert!(!is_block_texture_path(Path::new(
            "assets/minecraft/textures/entity/cow/cow.png"
        )));
        assert!(!is_block_texture_path(Path::new("block.png")));
        assert!(!is_block_texture_path(Path::new("items/block_item.png")));
    }

    #[test]
    fn colormap_paths_are_skipped() {
        assert!(is_colormap_texture_path("colormap/grass.png"));
        assert!(is_colormap_texture_path("colormap/foliage.png"));
        assert!(is_colormap_texture_path(
            "assets/minecraft/textures/colormap/grass.png"
        ));
        assert!(!is_colormap_texture_path("block/grass_block_side.png"));
        assert!(!is_colormap_texture_path("entity/grass/grass.png"));
    }

    #[test]
    fn batch_upscales_jar_into_resource_pack() {
        let root = std::env::temp_dir().join(format!(
            "xbrstudio-jar-batch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let model_dir = root.join("models");
        std::fs::create_dir_all(&model_dir).unwrap();
        let cow_model = json!({
            "model": "net.minecraft.client.model.animal.cow.CowModel",
            "method": "createBaseCowModel",
            "texture_size": [16, 16],
            "parts": [{
                "name": "body",
                "translation": [0, 0, 0],
                "rotation": [0, 0, 0],
                "cubes": [{
                    "origin": [0, 0, 0],
                    "size": [8, 8, 4],
                    "uv": [0, 0]
                }]
            }]
        });
        std::fs::write(
            model_dir.join("CowModel.createBaseCowModel.json"),
            cow_model.to_string(),
        )
        .unwrap();

        let wolf_model = json!({
            "model": "net.minecraft.client.model.animal.wolf.AdultWolfModel",
            "method": "createBodyLayer",
            "texture_size": [64, 32],
            "parts": [{
                "name": "body",
                "translation": [0, 0, 0],
                "rotation": [0, 0, 0],
                "cubes": [{
                    "origin": [0, 0, 0],
                    "size": [6, 6, 10],
                    "uv": [0, 0]
                }]
            }, {
                "name": "head",
                "translation": [0, 0, 0],
                "rotation": [0, 0, 0],
                "cubes": [{
                    "origin": [0, 0, 0],
                    "size": [6, 6, 6],
                    "uv": [0, 20]
                }]
            }]
        });
        std::fs::write(
            model_dir.join("AdultWolfModel.createBodyLayer.json"),
            wolf_model.to_string(),
        )
        .unwrap();

        let jar_path = root.join("test.jar");
        std::fs::write(&jar_path, build_test_jar()).unwrap();
        let out_dir = root.join("out");
        let opts = BatchOptions {
            factor: 2,
            stitch: true,
            model_dir: Some(model_dir),
            wrap: true,
        };

        let report = upscale_jar(&jar_path, &out_dir, &opts, |_| {}).unwrap();
        assert_eq!(report.total, 5, "colormap/grass.png must be skipped");
        assert_eq!(report.upscaled, 5, "errors: {:?}", report.errors);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!out_dir
            .join("assets/minecraft/textures/colormap/grass.png")
            .exists());
        assert_eq!(report.animated, 1);
        assert_eq!(report.stitched, 2, "cow + wolf should stitch");
        assert_eq!(report.wrapped, 2, "stone + lava (block/) should wrap");
        assert_eq!(report.entity_models, 2);
        assert_eq!(
            report.unmatched_entity,
            vec!["entity/equipment/test_armor.png".to_string()],
            "only the model-less armor texture is unmatched"
        );

        let stone = image::open(out_dir.join("assets/minecraft/textures/block/stone.png")).unwrap();
        assert_eq!((stone.width(), stone.height()), (4 * 2, 4 * 2));

        let lava =
            image::open(out_dir.join("assets/minecraft/textures/block/lava_still.png")).unwrap();
        assert_eq!((lava.width(), lava.height()), (4 * 2, 12 * 2));

        let cow = image::open(
            out_dir.join("assets/minecraft/textures/entity/cow/test_cow.png"),
        )
        .unwrap();
        assert_eq!((cow.width(), cow.height()), (16 * 2, 16 * 2));

        let armor = image::open(out_dir.join(
            "assets/minecraft/textures/entity/equipment/test_armor.png",
        ))
        .unwrap();
        assert_eq!((armor.width(), armor.height()), (16 * 2, 16 * 2));

        let lava_meta: Value = serde_json::from_str(
            &std::fs::read_to_string(
                out_dir.join("assets/minecraft/textures/block/lava_still.png.mcmeta"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(lava_meta["animation"]["width"], json!(8));
        assert_eq!(lava_meta["animation"]["height"], json!(8));
        assert_eq!(lava_meta["animation"]["frametime"], json!(2));

        let stone_meta = std::fs::read_to_string(
            out_dir.join("assets/minecraft/textures/block/stone.png.mcmeta"),
        )
        .unwrap();
        assert!(stone_meta.contains("mipmap_strategy"));

        let pack: Value = serde_json::from_str(
            &std::fs::read_to_string(out_dir.join("pack.mcmeta")).unwrap(),
        )
        .unwrap();
        assert_eq!(pack["pack"]["pack_format"], json!(97));

        assert!(!out_dir.join("version.json").exists());
        assert!(!out_dir.join("net/minecraft/Client.class").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
