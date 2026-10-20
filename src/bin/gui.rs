use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, ClickEvent, Context, PathPromptOptions, RenderImage, SharedString,
    TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div, img, prelude::*, px, rgb,
    size,
};
use image::{Rgba, RgbaImage};

use xbrstudio::jar::{
    self, BatchOptions, BatchProgress, JarMessage, is_wrap_texture_path, spawn_batch,
};
use xbrstudio::model::{ModelFaces, load_model};
use xbrstudio::pack;
use xbrstudio::preview3d;
use xbrstudio::scalers::{Algorithm, AlphaMode};
use xbrstudio::upscaling::{UpscaleConfig, upscale_image, upscale_wrapped};

actions!(xbrstudio, [Quit]);

/// The animation of a texture, read from its `.mcmeta` file.
#[derive(Debug, Clone, Copy)]
struct Animation {
    #[allow(dead_code)]
    frame_width: u32,
    frame_height: u32,
    /// The time one frame is shown, in milliseconds.
    #[allow(dead_code)]
    frame_time: u64,
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-q", Quit, None),
            gpui::KeyBinding::new("ctrl-q", Quit, None),
        ]);

        let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("XBR Studio")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| AppView::new()),
        )
        .unwrap();

        cx.activate(true);
    });
}

struct AppView {
    texture_path: Option<PathBuf>,
    model_label: String,
    model_faces: Option<ModelFaces>,
    source: Option<RgbaImage>,
    source_image: Option<Arc<RenderImage>>,
    result: Option<RgbaImage>,
    result_image: Option<Arc<RenderImage>>,
    /// The comparison view: the original on the left, the result on the
    /// right, split at `split`.
    compare_image: Option<Arc<RenderImage>>,
    factor: u32,
    algorithm: Algorithm,
    alpha: AlphaMode,
    sharpen: f32,
    stitch: bool,
    zoom: f32,
    /// Where the comparison is split, from 0 (all original) to 1 (all
    /// result).
    split: f32,
    status: String,
    error: Option<String>,
    batch_running: bool,
    batch_done: usize,
    batch_total: usize,
    batch_cancel: Option<Arc<AtomicBool>>,
    batch_dir: Option<PathBuf>,
    /// The models the picker offers.
    picker_open: bool,
    picker_models: Vec<String>,
    picker_source: Option<PathBuf>,
    /// The animation of the loaded texture.
    animation: Option<Animation>,
    frame: usize,
    /// The 3D preview.
    show_3d: bool,
    yaw: f32,
    pitch: f32,
    preview_image: Option<Arc<RenderImage>>,
}

impl AppView {
    fn new() -> Self {
        Self {
            texture_path: None,
            model_label: "—".into(),
            model_faces: None,
            source: None,
            source_image: None,
            result: None,
            result_image: None,
            compare_image: None,
            factor: 4,
            algorithm: Algorithm::Xbrz,
            alpha: AlphaMode::Straight,
            sharpen: 0.0,
            stitch: true,
            zoom: 4.0,
            split: 0.5,
            status: "Open a texture to begin.".into(),
            error: None,
            batch_running: false,
            batch_done: 0,
            batch_total: 0,
            batch_cancel: None,
            batch_dir: None,
            picker_open: false,
            picker_models: Vec::new(),
            picker_source: None,
            animation: None,
            frame: 0,
            show_3d: false,
            yaw: 0.6,
            pitch: 0.3,
            preview_image: None,
        }
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    fn clear_result(&mut self) {
        self.result = None;
        self.result_image = None;
        self.compare_image = None;
    }

    // ---------------------------------------------------------------- textures

    fn open_texture(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from("Open texture")),
        });
        cx.spawn(async move |this, cx| {
            let selected = match rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    this.update(cx, |view, cx| {
                        view.set_error(format!("file dialog: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    this.update(cx, |view, cx| {
                        view.set_error(format!("file dialog cancelled: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let Some(path) = selected else { return };
            this.update(cx, |view, cx| {
                view.load_texture(path);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_model(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from("Load model JSON")),
        });
        cx.spawn(async move |this, cx| {
            let selected = match rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    this.update(cx, |view, cx| {
                        view.set_error(format!("file dialog: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    this.update(cx, |view, cx| {
                        view.set_error(format!("file dialog cancelled: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let Some(path) = selected else { return };
            this.update(cx, |view, cx| {
                view.load_model_file(path);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_texture(&mut self, path: PathBuf) {
        match image::open(&path) {
            Ok(image) => {
                let rgba = image.to_rgba8();
                self.source_image = Some(to_render_image(&rgba));
                self.source = Some(rgba);
                self.texture_path = Some(path.clone());
                self.clear_result();
                self.error = None;
                self.frame = 0;
                self.animation = read_animation(&path);
                let (width, height) = self
                    .source
                    .as_ref()
                    .map(|image| (image.width(), image.height()))
                    .unwrap_or((0, 0));
                self.status = format!(
                    "Loaded {} ({width}x{height}).",
                    self.texture_path
                        .as_deref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default()
                );
            }
            Err(error) => self.set_error(format!("cannot read texture: {error}")),
        }
    }

    fn load_model_file(&mut self, path: PathBuf) {
        match load_model(&path) {
            Ok(faces) => {
                self.model_label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                self.set_model(faces);
                self.status = format!("Loaded model {}.", self.model_label);
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_model(&mut self, mut faces: ModelFaces) {
        if let Some(source) = &self.source {
            xbrstudio::model::scale_model_faces_to_image(&mut faces, source);
        }
        if faces.faces.is_empty() {
            self.set_error("the model has no face UV rectangles inside the texture");
            return;
        }
        self.model_faces = Some(faces);
        self.stitch = true;
        self.clear_result();
        self.update_preview();
    }

    // ------------------------------------------------------------ model picker

    fn toggle_picker(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.picker_open = !self.picker_open;
        if self.picker_open && self.picker_models.is_empty() {
            self.picker_models = list_entity_models();
        }
        cx.notify();
    }

    fn pick_model(&mut self, name: String, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.picker_source.clone() else {
            // An entity model from the model directory.
            let path = PathBuf::from("models/entity").join(&name);
            if !path.exists() {
                self.set_error(format!("no model named {name}"));
                cx.notify();
                return;
            }
            self.load_model_file(path);
            cx.notify();
            return;
        };

        match jar::load_block_model(&source, &name) {
            Ok(faces) => {
                self.model_label = name.clone();
                self.set_model(faces);
                self.status = format!("Loaded block model {name}.");
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
        cx.notify();
    }

    fn load_jar_models(&mut self, jar_path: PathBuf) {
        match jar::list_block_models(&jar_path) {
            Ok(mut names) => {
                names.extend(list_entity_models());
                names.sort();
                names.dedup();
                self.picker_models = names;
                self.picker_source = Some(jar_path);
                self.picker_open = true;
                self.status = format!("{} models available in the picker.", self.picker_models.len());
            }
            Err(error) => self.set_error(error),
        }
    }

    // ----------------------------------------------------------------- upscale

    fn set_factor(&mut self, factor: u32, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.factor != factor {
            self.factor = factor;
            self.clear_result();
            cx.notify();
        }
    }

    fn set_algorithm(
        &mut self,
        algorithm: Algorithm,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.algorithm != algorithm {
            self.algorithm = algorithm;
            self.clear_result();
            cx.notify();
        }
    }

    fn set_alpha(&mut self, alpha: AlphaMode, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.alpha != alpha {
            self.alpha = alpha;
            self.clear_result();
            cx.notify();
        }
    }

    fn set_sharpen(
        &mut self,
        sharpen: f32,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if (self.sharpen - sharpen).abs() > f32::EPSILON {
            self.sharpen = sharpen;
            self.clear_result();
            cx.notify();
        }
    }

    fn toggle_stitch(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.model_faces.is_some() {
            self.stitch = !self.stitch;
            self.clear_result();
            cx.notify();
        }
    }

    fn zoom_in(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.zoom = (self.zoom * 1.5).min(64.0);
        cx.notify();
    }

    fn zoom_out(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.zoom = (self.zoom / 1.5).max(0.25);
        cx.notify();
    }

    fn run_upscale(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            self.set_error("open a texture first");
            cx.notify();
            return;
        };

        let faces = self.model_faces.as_ref().map(|faces| faces.faces.clone());
        let stitch = self.stitch && faces.is_some();
        let wrap = !stitch
            && self
                .texture_path
                .as_deref()
                .is_some_and(is_wrap_texture_path);
        let config = UpscaleConfig {
            factor: self.factor,
            stitch_faces: stitch,
            algorithm: self.algorithm,
            alpha_mode: self.alpha,
            sharpen: self.sharpen,
        };

        let result = if wrap {
            upscale_wrapped(&source, &config)
        } else {
            upscale_image(&source, faces.as_deref(), &config)
        };
        match result {
            Ok(result) => {
                self.result_image = Some(to_render_image(&result));
                self.result = Some(result);
                self.update_compare();
                self.update_preview();
                let result_ref = self.result.as_ref().expect("just set");
                self.status = format!(
                    "Upscaled {}x{} -> {}x{} (factor {}, {})",
                    source.width(),
                    source.height(),
                    result_ref.width(),
                    result_ref.height(),
                    self.factor,
                    self.algorithm.name()
                );
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
        cx.notify();
    }

    /// The comparison image: the original (blown up to the size of the
    /// result) on the left, the result on the right.
    fn update_compare(&mut self) {
        let (Some(source), Some(result)) = (&self.source, &self.result) else {
            self.compare_image = None;
            return;
        };

        let (source, result) = match self.animation {
            Some(animation) => {
                let frame = self.frame % frame_count(animation, result.height());
                let y = frame as u32 * animation.frame_height;
                if y + animation.frame_height > result.height() {
                    (source.clone(), result.clone())
                } else {
                    (
                        crop_frame(source, animation, y),
                        crop_frame(result, animation, y),
                    )
                }
            }
            None => (source.clone(), result.clone()),
        };

        let width = result.width();
        let height = result.height();
        let mut image = RgbaImage::new(width, height);
        let split_x = (width as f32 * self.split) as u32;
        for y in 0..height {
            for x in 0..width {
                let original = sample_nearest(&source, x, y, width, height);
                let upscaled = *result.get_pixel(x, y);
                let pixel = if x < split_x { original } else { upscaled };
                image.put_pixel(x, y, pixel);
            }
        }
        self.compare_image = Some(to_render_image(&image));
    }

    fn set_split(&mut self, split: f32, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.split = split.clamp(0.0, 1.0);
        self.update_compare();
        cx.notify();
    }

    /// The 3D preview of the model.
    fn update_preview(&mut self) {
        if !self.show_3d {
            self.preview_image = None;
            return;
        }
        let Some(faces) = &self.model_faces else {
            self.preview_image = None;
            return;
        };
        let texture = self
            .result
            .as_ref()
            .or(self.source.as_ref())
            .cloned()
            .unwrap_or_else(|| RgbaImage::new(16, 16));
        let mesh = preview3d::build_mesh(faces);
        let image = preview3d::render(&mesh, &texture, 320, 320, self.yaw, self.pitch);
        self.preview_image = Some(to_render_image(&image));
    }

    fn toggle_3d(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_3d = !self.show_3d;
        self.update_preview();
        cx.notify();
    }

    fn rotate_3d(&mut self, dyaw: f32, dpitch: f32, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-1.4, 1.4);
        self.update_preview();
        cx.notify();
    }

    fn save_result(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.result.is_none() {
            self.set_error("run upscale first");
            cx.notify();
            return;
        }

        let suggested = self
            .texture_path
            .as_deref()
            .and_then(|path| path.file_stem())
            .map(|stem| format!("{}_x{}.png", stem.to_string_lossy(), self.factor))
            .unwrap_or_else(|| format!("upscaled_x{}.png", self.factor));
        let directory = self
            .texture_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let rx = cx.prompt_for_new_path(&directory, Some(&suggested));
        let this = cx.entity();
        window
            .spawn(cx, async move |cx| {
                let selected = match rx.await {
                    Ok(Ok(Some(path))) => Some(path),
                    Ok(Ok(None)) => None,
                    Ok(Err(error)) => {
                        this.update(cx, |view, cx| {
                            view.set_error(format!("file dialog: {error}"));
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                    Err(error) => {
                        this.update(cx, |view, cx| {
                            view.set_error(format!("file dialog cancelled: {error}"));
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                };
                let Some(path) = selected else { return };
                this.update(cx, |view, cx| {
                    view.write_result(&path);
                    cx.notify();
                })
                .ok();
            })
            .detach();
    }

    fn write_result(&mut self, path: &std::path::Path) {
        let Some(result) = &self.result else {
            self.set_error("run upscale first");
            return;
        };
        match result.save(path) {
            Ok(()) => {
                self.error = None;
                self.status = format!("Saved {}", path.display());
            }
            Err(error) => self.set_error(format!("cannot write {}: {error}", path.display())),
        }
    }

    // ------------------------------------------------------------------- batch

    fn open_jar(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_running {
            return;
        }
        self.batch_running = true;
        self.error = None;
        self.status = "Choose a Minecraft jar…".into();
        cx.notify();

        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from("Open Minecraft jar")),
        });
        cx.spawn(async move |this, cx| {
            let selected = match rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    this.update(cx, |view, cx| {
                        view.batch_running = false;
                        view.set_error(format!("file dialog: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    this.update(cx, |view, cx| {
                        view.batch_running = false;
                        view.set_error(format!("file dialog cancelled: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let Some(jar) = selected else {
                this.update(cx, |view, cx| {
                    view.batch_running = false;
                    view.status = "Jar batch cancelled.".into();
                    cx.notify();
                })
                .ok();
                return;
            };

            let out_rx = match cx.update(|app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some(SharedString::from(
                        "Output folder for the upscaled resource pack",
                    )),
                })
            }) {
                Ok(rx) => rx,
                Err(error) => {
                    this.update(cx, |view, cx| {
                        view.batch_running = false;
                        view.set_error(format!("file dialog: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let selected_out = match out_rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    this.update(cx, |view, cx| {
                        view.batch_running = false;
                        view.set_error(format!("file dialog: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    this.update(cx, |view, cx| {
                        view.batch_running = false;
                        view.set_error(format!("file dialog cancelled: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let Some(out_dir) = selected_out else {
                this.update(cx, |view, cx| {
                    view.batch_running = false;
                    view.status = "Jar batch cancelled.".into();
                    cx.notify();
                })
                .ok();
                return;
            };

            let opts = match this.update(cx, |view, cx| {
                view.status = format!("Scanning {}…", jar.display());
                cx.notify();
                BatchOptions {
                    factor: view.factor,
                    stitch: view.stitch,
                    model_dir: None,
                    wrap: true,
                    algorithm: view.algorithm,
                    alpha_mode: view.alpha,
                    sharpen: view.sharpen,
                    cancel: Some(Arc::new(AtomicBool::new(false))),
                    ..BatchOptions::default()
                }
            }) {
                Ok(opts) => opts,
                Err(_) => return,
            };
            let cancel = opts.cancel.clone();
            let factor = opts.factor;
            this.update(cx, |view, cx| {
                view.batch_cancel = cancel;
                view.batch_done = 0;
                view.batch_total = 0;
                view.batch_dir = Some(out_dir.clone());
                view.load_jar_models(jar.clone());
                cx.notify();
            })
            .ok();

            let receiver = spawn_batch(jar, out_dir.clone(), opts);

            loop {
                let mut finished = false;
                loop {
                    match receiver.try_recv() {
                        Ok(JarMessage::Progress(BatchProgress::Started {
                            total,
                            entity_models,
                            block_models,
                        })) => {
                            this.update(cx, |view, cx| {
                                view.batch_total = total;
                                view.status = format!(
                                    "Upscaling 0/{total} textures ({entity_models} entity, {block_models} block models)"
                                );
                                cx.notify();
                            })
                            .ok();
                        }
                        Ok(JarMessage::Progress(BatchProgress::Texture {
                            done,
                            total,
                            name,
                        })) => {
                            this.update(cx, |view, cx| {
                                view.batch_done = done;
                                view.batch_total = total;
                                view.status = format!("[{done}/{total}] {name}");
                                cx.notify();
                            })
                            .ok();
                        }
                        Ok(JarMessage::Finished(result)) => {
                            this.update(cx, |view, cx| {
                                view.batch_running = false;
                                view.batch_cancel = None;
                                match result {
                                    Ok(report) => {
                                        if report.upscaled == 0 && !report.errors.is_empty() {
                                            view.set_error(format!(
                                                "all {} textures failed: {}",
                                                report.errors.len(),
                                                report.errors[0].1
                                            ));
                                        } else {
                                            let mut summary = format!(
                                                "Upscaled {}/{} textures to {} (x{factor})",
                                                report.upscaled,
                                                report.upscaled + report.copied,
                                                out_dir.display()
                                            );
                                            if report.copied > 0 {
                                                summary.push_str(&format!(
                                                    ", {} colormap{} copied as-is",
                                                    report.copied,
                                                    if report.copied == 1 { "" } else { "s" }
                                                ));
                                            }
                                            if report.stitched > 0 {
                                                summary.push_str(&format!(
                                                    ", {} stitched",
                                                    report.stitched
                                                ));
                                            }
                                            if !report.errors.is_empty() {
                                                let (path, message) = &report.errors[0];
                                                summary.push_str(&format!(
                                                    "; {} failed (e.g. {path}: {message})",
                                                    report.errors.len()
                                                ));
                                            }
                                            view.status = summary;
                                            view.error = None;
                                        }
                                    }
                                    Err(error) => view.set_error(error),
                                }
                                cx.notify();
                            })
                            .ok();
                            finished = true;
                            break;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            this.update(cx, |view, cx| {
                                view.batch_running = false;
                                view.set_error("batch worker thread exited unexpectedly");
                                cx.notify();
                            })
                            .ok();
                            finished = true;
                            break;
                        }
                    }
                }
                if finished {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
            }
        })
        .detach();
    }

    fn cancel_batch(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.batch_cancel {
            cancel.store(true, Ordering::Relaxed);
            self.status = "Stopping the batch…".into();
        }
        cx.notify();
    }

    /// Zip the pack of the last batch.
    fn build_pack(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.batch_dir.clone() else {
            self.set_error("run a jar batch first");
            cx.notify();
            return;
        };
        let zip_path = dir.with_extension("zip");
        match pack::zip_directory(&dir, &zip_path) {
            Ok(()) => {
                self.status = format!("Wrote {}", zip_path.display());
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
        cx.notify();
    }

    /// Install the pack of the last batch into Minecraft.
    fn install_pack(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.batch_dir.clone() else {
            self.set_error("run a jar batch first");
            cx.notify();
            return;
        };
        let zip_path = dir.with_extension("zip");
        if !zip_path.is_file() && let Err(error) = pack::zip_directory(&dir, &zip_path) {
            self.set_error(error);
            cx.notify();
            return;
        }
        match pack::install_pack(&zip_path) {
            Ok(path) => {
                self.status = format!("Installed {}", path.display());
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
        cx.notify();
    }
}

/// The animation of a texture, read from the `.mcmeta` file next to it.
fn read_animation(texture_path: &Path) -> Option<Animation> {
    let meta_path = PathBuf::from(format!("{}.mcmeta", texture_path.display()));
    let data = std::fs::read_to_string(meta_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&data).ok()?;
    let animation = value.get("animation")?;
    let frame_time = animation
        .get("frametime")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1)
        * 50;

    let image = image::open(texture_path).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    let (frame_width, frame_height) = xbrstudio::jar::animation_frame_size(width, height, animation);
    Some(Animation {
        frame_width,
        frame_height,
        frame_time,
    })
}

fn frame_count(animation: Animation, height: u32) -> usize {
    if animation.frame_height == 0 {
        1
    } else {
        (height / animation.frame_height).max(1) as usize
    }
}

/// Cut one frame out of an animated texture.
fn crop_frame(image: &RgbaImage, animation: Animation, y: u32) -> RgbaImage {
    let height = animation.frame_height.min(image.height().saturating_sub(y));
    image::imageops::crop_imm(image, 0, y, image.width(), height).to_image()
}

/// Read a pixel of an image, scaled to the target size.
fn sample_nearest(image: &RgbaImage, x: u32, y: u32, width: u32, height: u32) -> Rgba<u8> {
    if image.width() == 0 || image.height() == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let sx = (x as u64 * image.width() as u64 / width.max(1) as u64) as u32;
    let sy = (y as u64 * image.height() as u64 / height.max(1) as u64) as u32;
    *image.get_pixel(sx.min(image.width() - 1), sy.min(image.height() - 1))
}

/// The entity models in the model directory.
fn list_entity_models() -> Vec<String> {
    let mut names = Vec::new();
    let root = PathBuf::from("models/entity");
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "json")
                && let Ok(relative) = path.strip_prefix(&root)
            {
                names.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    names.sort();
    names
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_source = self.source.is_some();
        let has_result = self.result.is_some();
        let has_model = self.model_faces.is_some();
        let stitch = self.stitch;
        let factor = self.factor;
        let algorithm = self.algorithm;
        let alpha = self.alpha;
        let sharpen = self.sharpen;
        let batch_running = self.batch_running;
        let batch_done = self.batch_done;
        let batch_total = self.batch_total;
        let picker_open = self.picker_open;
        let show_3d = self.show_3d;
        let zoom = self.zoom;
        let split = self.split;
        let compare_image = self.compare_image.clone();
        let _source_image = self.source_image.clone();
        let preview_image = self.preview_image.clone();
        let status_line = match &self.error {
            Some(error) => format!("error: {error}"),
            None => self.status.clone(),
        };
        let texture_label = self
            .texture_path
            .as_deref()
            .map(|path| format!("texture: {}", path.display()))
            .unwrap_or_else(|| "texture: —".into());
        let model_label = format!("model: {}", self.model_label);
        let face_count = self
            .model_faces
            .as_ref()
            .map(|m| m.faces.len())
            .unwrap_or(0);
        let model_names = self.picker_models.clone();
        let frame = self.frame;
        let frames = self
            .animation
            .map(|animation| {
                frame_count(
                    animation,
                    self.source.as_ref().map(|i| i.height()).unwrap_or(1),
                )
            })
            .unwrap_or(1);

        let open_texture = cx.listener(Self::open_texture);
        let open_model = cx.listener(Self::open_model);
        let open_jar = cx.listener(Self::open_jar);
        let toggle_picker = cx.listener(Self::toggle_picker);
        let toggle_stitch = cx.listener(Self::toggle_stitch);
        let run_upscale = cx.listener(Self::run_upscale);
        let save_result = cx.listener(Self::save_result);
        let build_pack = cx.listener(Self::build_pack);
        let install_pack = cx.listener(Self::install_pack);
        let toggle_3d = cx.listener(Self::toggle_3d);
        let zoom_in = cx.listener(Self::zoom_in);
        let zoom_out = cx.listener(Self::zoom_out);
        let cancel_batch = cx.listener(Self::cancel_batch);

        let factor_listeners: Vec<_> = [2u32, 3, 4, 5, 6]
            .into_iter()
            .map(|value| {
                let listener = cx.listener(move |view, event, window, cx| {
                    view.set_factor(value, event, window, cx);
                });
                (value, listener)
            })
            .collect();

        let algorithm_listeners: Vec<_> = [
            Algorithm::Xbrz,
            Algorithm::Fsr,
            Algorithm::Lanczos3,
            Algorithm::Bicubic,
        ]
        .into_iter()
        .map(|value| {
            let listener = cx.listener(move |view, event, window, cx| {
                view.set_algorithm(value, event, window, cx);
            });
            (value, listener)
        })
        .collect();

        let alpha_listeners: Vec<_> = [AlphaMode::Straight, AlphaMode::Premultiplied, AlphaMode::Masked]
            .into_iter()
            .map(|value| {
                let listener = cx.listener(move |view, event, window, cx| {
                    view.set_alpha(value, event, window, cx);
                });
                (value, listener)
            })
            .collect();

        let sharpen_listeners: Vec<_> = [0.0f32, 0.25, 0.5]
            .into_iter()
            .map(|value| {
                let listener = cx.listener(move |view, event, window, cx| {
                    view.set_sharpen(value, event, window, cx);
                });
                (value, listener)
            })
            .collect();

        let rotate_listeners: Vec<_> = [(-0.4f32, 0.0f32), (0.4, 0.0), (0.0, 0.3), (0.0, -0.3)]
            .into_iter()
            .map(|(yaw, pitch)| {
                let listener = cx.listener(move |view, event, window, cx| {
                    view.rotate_3d(yaw, pitch, event, window, cx);
                });
                (yaw, listener)
            })
            .collect();

        let split_listeners: Vec<_> = (0..=20)
            .map(|step| {
                let position = step as f32 / 20.0;
                let listener = cx.listener(move |view, event, window, cx| {
                    view.set_split(position, event, window, cx);
                });
                (step, listener)
            })
            .collect();

        let model_listeners: Vec<_> = model_names
            .iter()
            .map(|name| {
                let owned = name.clone();
                let listener = cx.listener(move |view, event, window, cx| {
                    view.pick_model(owned.clone(), event, window, cx);
                });
                (name.clone(), listener)
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xe0e0e0))
            .child(
                // Toolbar
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(0x3c3c3c))
                    .child(toolbar_button("Open texture…", true, open_texture))
                    .child(toolbar_button("Open model…", true, open_model))
                    .child(toolbar_button("Open jar…", !batch_running, open_jar))
                    .child(toolbar_button(
                        if picker_open { "Hide models" } else { "Models…" },
                        true,
                        toggle_picker,
                    ))
                    .child(div().w(px(8.)))
                    .children(factor_listeners.into_iter().map(|(value, listener)| {
                        toolbar_button(
                            format!("{value}×"),
                            factor != value,
                            listener,
                        )
                    }))
                    .child(div().w(px(8.)))
                    .children(algorithm_listeners.into_iter().map(|(value, listener)| {
                        toolbar_button(value.name(), algorithm != value, listener)
                    }))
                    .child(div().w(px(8.)))
                    .children(alpha_listeners.into_iter().map(|(value, listener)| {
                        toolbar_button(value.name(), alpha != value, listener)
                    }))
                    .child(div().w(px(8.)))
                    .children(sharpen_listeners.into_iter().map(|(value, listener)| {
                        toolbar_button(
                            format!("sharp {:.2}", value),
                            (sharpen - value).abs() > f32::EPSILON,
                            listener,
                        )
                    }))
                    .child(div().w(px(8.)))
                    .child(stitch_toggle(has_model && stitch, has_model, toggle_stitch))
                    .child(toolbar_button("Upscale", has_source, run_upscale))
                    .child(toolbar_button("Save…", has_result, save_result))
                    .child(toolbar_button("Build pack", true, build_pack))
                    .child(toolbar_button("Install", true, install_pack))
                    .child(toolbar_button(
                        if show_3d { "Hide 3D" } else { "3D preview" },
                        has_model,
                        toggle_3d,
                    )),
            )
            .child(
                // Body
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .when(picker_open, |this| {
                        this.child(
                            div()
                                .id("model-picker")
                                .flex()
                                .flex_col()
                                .w(px(220.))
                                .border_r_1()
                                .border_color(rgb(0x3c3c3c))
                                .overflow_y_scroll()
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .text_color(rgb(0x9a9a9a))
                                        .child(format!("{} models", model_names.len())),
                                )
                                .children(model_listeners.into_iter().map(|(name, listener)| {
                                    div()
                                        .id(SharedString::from(format!("model-{name}")))
                                        .px_2()
                                        .py_1()
                                        .cursor_pointer()
                                        .text_color(rgb(0xd0d0d0))
                                        .hover(|style| style.bg(rgb(0x333333)))
                                        .on_click(listener)
                                        .child(name)
                                })),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .px_2()
                                    .pt_2()
                                    .child(
                                        div()
                                            .text_color(rgb(0xb0b0b0))
                                            .child(format!(
                                                "{texture_label}    {model_label}    faces: {face_count}    frame: {}/{}",
                                                frame + 1,
                                                frames
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_1()
                                            .min_h(px(120.))
                                            .mt_2()
                                            .bg(rgb(0x141414))
                                            .border_1()
                                            .border_color(rgb(0x404040))
                                            .rounded_sm()
                                            .p_2()
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .size_full()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .when_some(compare_image.clone(), |this, image| {
                                                        this.child(
                                                            img(image)
                                                                .id("compare")
                                                                .object_fit(gpui::ObjectFit::Contain)
                                                                .w(px(16. * zoom.max(1.0)))
                                                                .h(px(16. * zoom.max(1.0))),
                                                        )
                                                    })
                                                    .when(compare_image.is_none(), |this| {
                                                        this.child(
                                                            div()
                                                                .text_color(rgb(0x606060))
                                                                .child("Upscale a texture to compare it"),
                                                        )
                                                    }),
                                            ),
                                    )
                                    .child(
                                        // The comparison slider.
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .mt_2()
                                            .child(div().text_color(rgb(0x808080)).child("original"))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .gap_1()
                                                    .children(split_listeners.into_iter().map(
                                                        |(step, listener)| {
                                                            let position = step as f32 / 20.0;
                                                            let selected = (position - split).abs() < 0.026;
                                                            div()
                                                                .id(SharedString::from(format!("split-{step}")))
                                                                .w(px(14.))
                                                                .h(px(14.))
                                                                .rounded_sm()
                                                                .cursor_pointer()
                                                                .border_1()
                                                                .border_color(rgb(0x505050))
                                                                .bg(if selected {
                                                                    rgb(0x5090d0)
                                                                } else {
                                                                    rgb(0x2a2a2a)
                                                                })
                                                                .hover(|style| style.bg(rgb(0x404040)))
                                                                .on_click(listener)
                                                        },
                                                    )),
                                            )
                                            .child(div().text_color(rgb(0x808080)).child("upscaled"))
                                            .child(div().flex_1())
                                            .child(toolbar_button("−", true, zoom_out))
                                            .child(toolbar_button("+", true, zoom_in)),
                                    ),
                            )
                            .when(show_3d, |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_2()
                                        .p_2()
                                        .border_t_1()
                                        .border_color(rgb(0x3c3c3c))
                                        .child(
                                            div()
                                                .w(px(320.))
                                                .h(px(320.))
                                                .bg(rgb(0x141414))
                                                .border_1()
                                                .border_color(rgb(0x404040))
                                                .rounded_sm()
                                                .overflow_hidden()
                                                .when_some(preview_image.clone(), |this, image| {
                                                    this.child(
                                                        img(image)
                                                            .id("preview3d")
                                                            .object_fit(gpui::ObjectFit::Contain)
                                                            .size_full(),
                                                    )
                                                }),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .children(rotate_listeners.into_iter().map(
                                                    |(_yaw, listener)| {
                                                        toolbar_button("rotate", true, listener)
                                                    },
                                                )),
                                        ),
                                )
                            }),
                    ),
            )
            .child(
                // Status bar
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .border_t_1()
                    .border_color(rgb(0x3c3c3c))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(0x9a9a9a))
                            .child(status_line),
                    )
                    .when(batch_total > 0, |this| {
                        this.child(
                            div()
                                .w(px(160.))
                                .h(px(10.))
                                .bg(rgb(0x2a2a2a))
                                .rounded_sm()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(px(160.0 * batch_done as f32 / batch_total.max(1) as f32))
                                        .bg(rgb(0x5090d0)),
                                ),
                        )
                    })
                    .when(batch_running, |this| {
                        this.child(toolbar_button("Cancel", true, cancel_batch))
                    }),
            )
    }
}

fn toolbar_button(
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    let id = SharedString::from(format!("btn-{label}"));
    let button = div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x5a5a5a))
        .bg(rgb(0x3c3c3c))
        .text_color(rgb(0xe8e8e8));

    if enabled {
        button
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x4a4a4a)))
            .on_click(on_click)
    } else {
        button.text_color(rgb(0x6a6a6a))
    }
}

fn stitch_toggle(
    enabled: bool,
    has_model: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let row = div()
        .id("stitch-toggle")
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x5a5a5a))
        .bg(rgb(0x3c3c3c))
        .child(box_el())
        .child("Stitch faces");

    if enabled && has_model {
        row.cursor_pointer()
            .hover(|style| style.bg(rgb(0x4a4a4a)))
            .on_click(on_click)
    } else {
        row.text_color(rgb(0x6a6a6a))
    }
}

fn box_el() -> impl IntoElement {
    div()
        .w(px(12.))
        .h(px(12.))
        .border_1()
        .border_color(rgb(0x909090))
        .rounded_sm()
}

fn to_render_image(rgba: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = rgba.clone();
    for pixel in (&mut *bgra).chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let frame = image::Frame::new(bgra);
    Arc::new(RenderImage::new(vec![frame]))
}
