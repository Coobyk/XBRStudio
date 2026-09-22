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
    /// Upscale `textures/block/**` with a 1px self-tiling border so edges get
    /// wrap context (border scaled away afterwards).
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

struct EntityModel {
    faces: ModelFaces,
    has_texture_size: bool,
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
        HashMap::new()
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
    models: &HashMap<String, EntityModel>,
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
    let wrap = opts.wrap && relative.starts_with("block/");

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
            if let Some(family) = entity_family_key(relative) {
                if let Some(template) = models.get(&family) {
                    let mut model_faces = template.faces.clone();
                    if !template.has_texture_size {
                        model_faces.uv_size = (
                            image.width() as f32,
                            image.height() as f32,
                        );
                    }
                    scale_model_faces_to_image(&mut model_faces, &image);
                    if !model_faces.faces.is_empty()
                        && model_faces.faces.len() * 2 >= template.faces.faces.len()
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

fn model_score(method: &str, has_texture_size: bool, face_count: usize) -> i64 {
    let method = method.to_ascii_lowercase();
    let mut score = face_count as i64;
    if has_texture_size {
        score += 2_000;
    }
    if method.contains("armor") {
        score -= 10_000;
    }
    if method.contains("chest") || method.contains("pose") {
        score -= 500;
    }
    score
}

fn load_entity_models(model_dir: &Path) -> HashMap<String, EntityModel> {
    let mut json_files = Vec::new();
    collect_json_files(model_dir, &mut json_files);

    let mut models: HashMap<String, (i64, EntityModel)> = HashMap::new();
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
            .unwrap_or_default();
        let score = model_score(method, has_texture_size, parsed.faces.len());

        match models.get(&key) {
            Some((best, _)) if *best >= score => {}
            _ => {
                models.insert(
                    key,
                    (
                        score,
                        EntityModel {
                            faces: parsed,
                            has_texture_size,
                        },
                    ),
                );
            }
        }
    }

    models
        .into_iter()
        .map(|(key, (_, model))| (key, model))
        .collect()
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
        let files: [(&str, Vec<u8>); 8] = [
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
                "assets/minecraft/textures/entity/equipment/test_armor.png",
                png_bytes(16, 16, [90, 90, 90, 255]),
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
    fn base_meshes_outrank_armor_and_pose_variants() {
        assert!(
            model_score("createBodyLayer", true, 48)
                > model_score("createBaseArmorMesh", true, 24)
        );
        assert!(
            model_score("addCommonParts", true, 54)
                > model_score("createChestBoatModel", true, 72)
        );
        assert!(
            model_score("createBodyLayer", true, 54)
                > model_score("createSittingPoseBodyLayer", true, 66)
        );
        assert!(model_score("createMesh", false, 42) > 0);
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
        assert_eq!(report.total, 4);
        assert_eq!(report.upscaled, 4, "errors: {:?}", report.errors);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.animated, 1);
        assert_eq!(report.stitched, 1);
        assert_eq!(report.wrapped, 2, "stone + lava (block/) should wrap");
        assert_eq!(report.entity_models, 1);

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
