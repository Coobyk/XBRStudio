use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, ClickEvent, Context, Div, FontWeight, PathPromptOptions, RenderImage,
    SharedString, Stateful, Task, TitlebarOptions, Window, WindowBounds, WindowOptions, actions,
    div, img, prelude::*, px, rgb, size,
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
    frame_time: u64,
}

fn main() {
    // A texture to open right away, so the app can be started with one.
    let texture = std::env::args().nth(1).map(PathBuf::from);

    // Use the GPU for the scaling when there is one. Without a device the
    // work stays on the CPU, so this never fails.
    #[cfg(feature = "gpu")]
    let _ = xbrstudio::gpu::init();

    Application::new().run(|cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-q", Quit, None),
            gpui::KeyBinding::new("ctrl-q", Quit, None),
        ]);

        let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("XBR Studio")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| {
                    let mut view = AppView::new();
                    if let Some(path) = texture {
                        view.load_texture(path, cx);
                    }
                    view
                })
            },
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
    /// The task that moves the animation on, if there is one.
    animation_task: Option<Task<()>>,
    frame: usize,
    /// The 3D preview.
    show_3d: bool,
    yaw: f32,
    pitch: f32,
    preview_image: Option<Arc<RenderImage>>,
    /// True while the app is scaling in the background.
    working: bool,
    /// The number of the running upscale, so the result of an old one can be
    /// thrown away.
    job: u64,
    /// The same for the 3D preview.
    preview_job: u64,
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
            animation_task: None,
            frame: 0,
            show_3d: false,
            yaw: 0.6,
            pitch: 0.3,
            preview_image: None,
            working: false,
            job: 0,
            preview_job: 0,
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
                view.load_texture(path, cx);
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
                view.load_model_file(path, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_texture(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match image::open(&path) {
            Ok(image) => {
                let rgba = image.to_rgba8();
                let (width, height) = (rgba.width(), rgba.height());
                self.source_image = Some(to_render_image(&rgba));
                self.source = Some(rgba);
                self.texture_path = Some(path.clone());
                self.clear_result();
                self.error = None;
                self.frame = 0;
                self.animation = read_animation(&path);
                self.zoom = fit_zoom(width, height);
                self.start_animation(cx);
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

    /// Move the animation on, one frame at a time.
    ///
    /// The task is replaced whenever another texture is loaded, so the old
    /// one stops.
    fn start_animation(&mut self, cx: &mut Context<Self>) {
        // Dropping the task cancels it.
        self.animation_task = None;

        let Some(animation) = self.animation else {
            return;
        };
        let frames = self
            .source
            .as_ref()
            .map_or(0, |image| frame_count(animation, image.height()));
        if frames <= 1 {
            return;
        }
        // Never go faster than twenty frames a second.
        let frame_time = animation.frame_time.clamp(50, 1000);

        self.animation_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(frame_time))
                    .await;
                let Ok(keep_going) = this.update(cx, |view, cx| {
                    let Some(animation) = view.animation else {
                        return false;
                    };
                    let frames = view
                        .source
                        .as_ref()
                        .map_or(0, |image| frame_count(animation, image.height()));
                    if frames <= 1 {
                        return false;
                    }
                    view.frame = (view.frame + 1) % frames;
                    cx.notify();
                    true
                }) else {
                    break;
                };
                if !keep_going {
                    break;
                }
            }
        }));
    }

    fn load_model_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match load_model(&path) {
            Ok(faces) => {
                self.model_label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                self.set_model(faces, cx);
                self.status = format!("Loaded model {}.", self.model_label);
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_model(&mut self, mut faces: ModelFaces, cx: &mut Context<Self>) {
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
        self.update_preview(cx);
    }

    // ------------------------------------------------------------ model picker

    fn toggle_picker(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.picker_open = !self.picker_open;
        if self.picker_open && self.picker_models.is_empty() {
            self.picker_models = list_entity_models();
        }
        cx.notify();
    }

    fn pick_model(
        &mut self,
        name: String,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.picker_source.clone() else {
            // An entity model from the model directory.
            let path = PathBuf::from("models/entity").join(&name);
            if !path.exists() {
                self.set_error(format!("no model named {name}"));
                cx.notify();
                return;
            }
            self.load_model_file(path, cx);
            cx.notify();
            return;
        };

        match jar::load_block_model(&source, &name) {
            Ok(faces) => {
                self.model_label = name.clone();
                self.set_model(faces, cx);
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
                self.status = format!(
                    "{} models available in the picker.",
                    self.picker_models.len()
                );
            }
            Err(error) => self.set_error(error),
        }
    }

    // ----------------------------------------------------------------- upscale

    fn set_factor(
        &mut self,
        factor: u32,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn set_alpha(
        &mut self,
        alpha: AlphaMode,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        // The scaling happens on a thread of its own, so the window stays
        // alive while it runs.
        let (source_width, source_height) = (source.width(), source.height());
        let split = self.split;
        self.job += 1;
        let job = self.job;
        self.working = true;
        self.status = "Upscaling…".into();
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = if wrap {
                        upscale_wrapped(&source, &config)
                    } else {
                        upscale_image(&source, faces.as_deref(), &config)
                    };
                    result.map(|result| {
                        let compare = build_compare(&source, &result, split);
                        (result, compare)
                    })
                })
                .await;

            this.update(cx, |view, cx| {
                if view.job != job {
                    // A newer job is already running.
                    return;
                }
                view.working = false;
                match result {
                    Ok((result, compare)) => {
                        view.result_image = Some(to_render_image(&result));
                        view.result = Some(result);
                        view.compare_image = Some(to_render_image(&compare));
                        view.update_preview(cx);
                        let result_ref = view.result.as_ref().expect("just set");
                        view.status = format!(
                            "Upscaled {}x{} -> {}x{} (factor {}, {})",
                            source_width,
                            source_height,
                            result_ref.width(),
                            result_ref.height(),
                            view.factor,
                            view.algorithm.name()
                        );
                        view.error = None;
                    }
                    Err(error) => view.set_error(error),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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

        let compare = build_compare(&source, &result, self.split);
        self.compare_image = Some(to_render_image(&compare));
    }

    fn set_split(
        &mut self,
        split: f32,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split = split.clamp(0.0, 1.0);
        self.update_compare();
        cx.notify();
    }

    /// The 3D preview of the model.
    ///
    /// The picture is rendered on a thread of its own, the software
    /// rasteriser takes a moment.
    fn update_preview(&mut self, cx: &mut Context<Self>) {
        if !self.show_3d {
            self.preview_image = None;
            return;
        }
        let Some(faces) = self.model_faces.clone() else {
            self.preview_image = None;
            return;
        };
        let texture = self
            .result
            .as_ref()
            .or(self.source.as_ref())
            .cloned()
            .unwrap_or_else(|| RgbaImage::new(16, 16));
        let yaw = self.yaw;
        let pitch = self.pitch;

        self.preview_job += 1;
        let job = self.preview_job;

        cx.spawn(async move |this, cx| {
            let image = cx
                .background_executor()
                .spawn(async move {
                    let mesh = preview3d::build_mesh(&faces);
                    preview3d::render(&mesh, &texture, 320, 320, yaw, pitch)
                })
                .await;

            this.update(cx, |view, cx| {
                if view.preview_job != job {
                    return;
                }
                view.preview_image = Some(to_render_image(&image));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_3d(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_3d = !self.show_3d;
        self.update_preview(cx);
        cx.notify();
    }

    fn rotate_3d(
        &mut self,
        dyaw: f32,
        dpitch: f32,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-1.4, 1.4);
        self.update_preview(cx);
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
                // Take a copy of the result and write it on a thread of its
                // own, a big texture takes a moment.
                let Some(result) = this
                    .update(cx, |view, _cx| view.result.clone())
                    .ok()
                    .flatten()
                else {
                    this.update(cx, |view, cx| {
                        view.set_error("run upscale first");
                        cx.notify();
                    })
                    .ok();
                    return;
                };
                this.update(cx, |view, cx| {
                    view.working = true;
                    view.status = "Saving…".into();
                    cx.notify();
                })
                .ok();
                let saved_path = path.clone();
                let written = cx
                    .background_executor()
                    .spawn(async move { result.save(&path) })
                    .await;
                this.update(cx, |view, cx| {
                    view.working = false;
                    match written {
                        Ok(()) => {
                            view.error = None;
                            view.status = format!("Saved {}", saved_path.display());
                        }
                        Err(error) => view
                            .set_error(format!("cannot write {}: {error}", saved_path.display())),
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
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
                // The worker sends a message for every texture. Only the
                // last one is drawn, so the window is not redrawn thousands
                // of times.
                let mut latest: Option<BatchProgress> = None;
                loop {
                    match receiver.try_recv() {
                        Ok(JarMessage::Progress(progress)) => {
                            latest = Some(progress);
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
                // Draw the progress of the batch, but only once for all the
                // textures that came in since the last time.
                if let Some(progress) = latest {
                    this.update(cx, |view, cx| {
                        match progress {
                            BatchProgress::Started {
                                total,
                                entity_models,
                                block_models,
                            } => {
                                view.batch_total = total;
                                view.status = format!(
                                    "Upscaling 0/{total} textures ({entity_models} entity, {block_models} block models)"
                                );
                            }
                            BatchProgress::Texture { done, total, name } => {
                                view.batch_done = done;
                                view.batch_total = total;
                                view.status = format!("[{done}/{total}] {name}");
                            }
                        }
                        cx.notify();
                    })
                    .ok();
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
    ///
    /// A big pack takes a while to zip, so it happens on a thread of its
    /// own.
    fn build_pack(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.batch_dir.clone() else {
            self.set_error("run a jar batch first");
            cx.notify();
            return;
        };
        let zip_path = dir.with_extension("zip");
        let written_path = zip_path.clone();
        self.working = true;
        self.status = "Zipping the pack…".into();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { pack::zip_directory(&dir, &zip_path) })
                .await;

            this.update(cx, |view, cx| {
                view.working = false;
                match result {
                    Ok(()) => {
                        view.status = format!("Wrote {}", written_path.display());
                        view.error = None;
                    }
                    Err(error) => view.set_error(error),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Install the pack of the last batch into Minecraft.
    fn install_pack(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.batch_dir.clone() else {
            self.set_error("run a jar batch first");
            cx.notify();
            return;
        };
        let zip_path = dir.with_extension("zip");
        self.working = true;
        self.status = "Installing the pack…".into();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if !zip_path.is_file() {
                        pack::zip_directory(&dir, &zip_path)?;
                    }
                    pack::install_pack(&zip_path)
                })
                .await;

            this.update(cx, |view, cx| {
                view.working = false;
                match result {
                    Ok(path) => {
                        view.status = format!("Installed {}", path.display());
                        view.error = None;
                    }
                    Err(error) => view.set_error(error),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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
    let (frame_width, frame_height) =
        xbrstudio::jar::animation_frame_size(width, height, animation);
    Some(Animation {
        frame_width,
        frame_height,
        frame_time,
    })
}

fn frame_count(animation: Animation, height: u32) -> usize {
    height
        .checked_div(animation.frame_height)
        .map(|count| count.max(1) as usize)
        .unwrap_or(1)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_source = self.source.is_some();
        let has_result = self.result.is_some();
        let has_model = self.model_faces.is_some();
        let stitch = self.stitch;
        let factor = self.factor;
        let algorithm = self.algorithm;
        let alpha = self.alpha;
        let sharpen = self.sharpen;
        let batch_running = self.batch_running;
        let working = self.working;
        let batch_done = self.batch_done;
        let batch_total = self.batch_total;
        let picker_open = self.picker_open;
        let show_3d = self.show_3d;
        let zoom = self.zoom;
        let split = self.split;
        let compare_image = self.compare_image.clone();
        let _source_image = self.source_image.clone();
        let preview_image = self.preview_image.clone();
        let (source_width, source_height) = self
            .source
            .as_ref()
            .map(|image| (image.width(), image.height()))
            .unwrap_or((16, 16));
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

        // An animated texture needs a new frame on the screen all the time.
        if frames > 1 {
            window.request_animation_frame();
        }

        let open_texture = cx.listener(Self::open_texture);
        let open_model = cx.listener(Self::open_model);
        let open_jar = cx.listener(Self::open_jar);
        // The welcome screen has its own buttons.
        let welcome_open_texture = cx.listener(Self::open_texture);
        let welcome_open_jar = cx.listener(Self::open_jar);
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
            Algorithm::Nearest,
        ]
        .into_iter()
        .map(|value| {
            let listener = cx.listener(move |view, event, window, cx| {
                view.set_algorithm(value, event, window, cx);
            });
            (value, listener)
        })
        .collect();

        let alpha_listeners: Vec<_> = [
            AlphaMode::Straight,
            AlphaMode::Premultiplied,
            AlphaMode::Masked,
        ]
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
            .bg(rgb(0x161616))
            .text_color(rgb(0xe0e0e0))
            .child(
                // The title of the window.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x2a2a2a))
                    .bg(rgb(0x1b1b1b))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf2f2f2))
                            .child("XBR Studio"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x6f6f6f))
                            .child("Minecraft textures, upscaled properly"),
                    )
                    .child(div().flex_1()),
            )
            .child(
                // Toolbar
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x2a2a2a))
                    .bg(rgb(0x1a1a1a))
                    .child(toolbar_group(
                        "File",
                        vec![
                            toolbar_button("Open texture…", false, true, open_texture),
                            toolbar_button("Open model…", false, true, open_model),
                            toolbar_button("Open jar…", false, !batch_running, open_jar),
                        ],
                    ))
                    .child(toolbar_group(
                        "Scale",
                        factor_listeners
                            .into_iter()
                            .map(|(value, listener)| {
                                toolbar_button(format!("{value}×"), factor == value, true, listener)
                            })
                            .collect(),
                    ))
                    .child(toolbar_group(
                        "Algorithm",
                        algorithm_listeners
                            .into_iter()
                            .map(|(value, listener)| {
                                toolbar_button(value.name(), algorithm == value, true, listener)
                            })
                            .collect(),
                    ))
                    .child(toolbar_group(
                        "Alpha",
                        alpha_listeners
                            .into_iter()
                            .map(|(value, listener)| {
                                toolbar_button(value.name(), alpha == value, true, listener)
                            })
                            .collect(),
                    ))
                    .child(toolbar_group(
                        "Sharpen",
                        sharpen_listeners
                            .into_iter()
                            .map(|(value, listener)| {
                                toolbar_button(
                                    format!("{value:.2}"),
                                    (sharpen - value).abs() < f32::EPSILON,
                                    true,
                                    listener,
                                )
                            })
                            .collect(),
                    ))
                    .child(toolbar_group(
                        "Model",
                        vec![
                            toolbar_button(
                                if picker_open {
                                    "Hide models"
                                } else {
                                    "Models…"
                                },
                                picker_open,
                                true,
                                toggle_picker,
                            ),
                            stitch_toggle(stitch, has_model, toggle_stitch),
                            toolbar_button(
                                if show_3d { "Hide 3D" } else { "3D preview" },
                                show_3d,
                                has_model,
                                toggle_3d,
                            ),
                        ],
                    ))
                    .child(toolbar_group(
                        "Actions",
                        vec![
                            toolbar_button("Upscale", false, has_source && !working, run_upscale),
                            toolbar_button("Save…", false, has_result, save_result),
                            toolbar_button("Build pack", false, true, build_pack),
                            toolbar_button("Install", false, true, install_pack),
                        ],
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
                                        .text_sm()
                                        .text_color(rgb(0xd0d0d0))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(0x333333)))
                                        .on_click(listener)
                                        // Only the file name, the folder is
                                        // the same for all of them.
                                        .child(
                                            Path::new(&name)
                                                .file_name()
                                                .map(|name| name.to_string_lossy().to_string())
                                                .unwrap_or_else(|| name.clone()),
                                        )
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
                                    .px_3()
                                    .pt_2()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_3()
                                            .text_sm()
                                            .text_color(rgb(0x8a8a8a))
                                            .child(texture_label.clone())
                                            .child(model_label.clone())
                                            .child(format!("faces: {face_count}"))
                                            .child(format!("frame: {}/{}", frame + 1, frames)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_1()
                                            .min_h(px(120.))
                                            .mt_2()
                                            .bg(rgb(0x111111))
                                            .border_1()
                                            .border_color(rgb(0x2e2e2e))
                                            .rounded_md()
                                            .p_2()
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .size_full()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .when_some(
                                                        compare_image.clone(),
                                                        |this, image| {
                                                            this.child(
                                                                img(image)
                                                                    .id("compare")
                                                                    .object_fit(
                                                                        gpui::ObjectFit::Contain,
                                                                    )
                                                                    .w(px(source_width as f32
                                                                        * zoom.max(0.25)))
                                                                    .h(px(source_height as f32
                                                                        * zoom.max(0.25))),
                                                            )
                                                        },
                                                    )
                                                    .when(!has_source, |this| {
                                                        this.child(welcome_panel(
                                                            welcome_open_texture,
                                                            welcome_open_jar,
                                                        ))
                                                    })
                                                    .when(
                                                        has_source && compare_image.is_none(),
                                                        |this| {
                                                            this.child(hint_panel(
                                                                "Press Upscale to see the result",
                                                            ))
                                                        },
                                                    ),
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
                                            .child(
                                                div().text_color(rgb(0x808080)).child("original"),
                                            )
                                            .child(
                                                div()
                                                    .relative()
                                                    .w(px(240.))
                                                    .h(px(18.))
                                                    .flex()
                                                    .items_center()
                                                    // The track and the part of it
                                                    // that is already upscaled.
                                                    .child(
                                                        div()
                                                            .w(px(240.))
                                                            .h(px(6.))
                                                            .rounded_full()
                                                            .bg(rgb(0x2e2e2e))
                                                            .child(
                                                                div()
                                                                    .h_full()
                                                                    .w(px(240.0 * split))
                                                                    .rounded_full()
                                                                    .bg(rgb(0x3c6ea8)),
                                                            ),
                                                    )
                                                    // The handle.
                                                    .child(
                                                        div()
                                                            .absolute()
                                                            .left(px(240.0 * split - 7.))
                                                            .w(px(14.))
                                                            .h(px(14.))
                                                            .rounded_full()
                                                            .bg(rgb(0x8ab6e8)),
                                                    )
                                                    // The steps that can be
                                                    // clicked.
                                                    .children(split_listeners.into_iter().map(
                                                        |(step, listener)| {
                                                            div()
                                                                .id(SharedString::from(format!(
                                                                    "split-{step}"
                                                                )))
                                                                .w(px(240. / 20.))
                                                                .h(px(18.))
                                                                .cursor_pointer()
                                                                .on_click(listener)
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div().text_color(rgb(0x808080)).child("upscaled"),
                                            )
                                            .child(div().flex_1())
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(0x808080))
                                                    .child(format!("×{zoom:.0}")),
                                            )
                                            .child(toolbar_button("−", false, true, zoom_out))
                                            .child(toolbar_button("+", false, true, zoom_in)),
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
                                        .child(div().flex().flex_col().gap_2().children(
                                            rotate_listeners.into_iter().map(|(_yaw, listener)| {
                                                toolbar_button("Rotate", false, true, listener)
                                            }),
                                        )),
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
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_color(rgb(0x9a9a9a))
                            .child(status_line),
                    )
                    // A little dot next to the status while the app works.
                    .when(working, |this| {
                        this.child(div().w(px(10.)).h(px(10.)).rounded_full().bg(rgb(0x5b93d4)))
                    })
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
                                        .w(
                                            px(160.0 * batch_done as f32
                                                / batch_total.max(1) as f32),
                                        )
                                        .bg(rgb(0x5090d0)),
                                ),
                        )
                    })
                    .when(batch_running, |this| {
                        this.child(toolbar_button("Cancel", false, true, cancel_batch))
                    }),
            )
    }
}

/// A button in the toolbar.
///
/// `selected` marks the button of the setting that is in use, `enabled` says
/// whether the button can be pressed at all.
fn toolbar_button(
    label: impl Into<SharedString>,
    selected: bool,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label = label.into();
    let id = SharedString::from(format!("btn-{label}"));

    let background = if selected {
        rgb(0x3c6ea8)
    } else if enabled {
        rgb(0x2e2e2e)
    } else {
        rgb(0x242424)
    };
    let text = if selected {
        rgb(0xffffff)
    } else if enabled {
        rgb(0xd6d6d6)
    } else {
        rgb(0x636363)
    };

    let button = div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(if selected {
            rgb(0x5b93d4)
        } else {
            rgb(0x3a3a3a)
        })
        .bg(background)
        .text_color(text)
        .child(label);

    if enabled {
        button
            .cursor_pointer()
            .hover(|style| {
                style.bg(if selected {
                    rgb(0x4a80c0)
                } else {
                    rgb(0x3a3a3a)
                })
            })
            .on_click(on_click)
    } else {
        button
    }
}

/// A group of buttons with a small caption above them.
fn toolbar_group(label: &str, items: Vec<impl IntoElement>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .max_w(px(460.))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x767676))
                .child(label.to_uppercase()),
        )
        .child(div().flex().flex_row().flex_wrap().gap_1().children(items))
}

/// The screen that is shown when nothing is open yet.
fn welcome_panel(
    open_texture: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    open_jar: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .px_4()
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0xf2f2f2))
                .child("XBR Studio"),
        )
        .child(
            div()
                .text_lg()
                .text_color(rgb(0xa8a8a8))
                .child("Minecraft textures, upscaled properly"),
        )
        .child(
            div()
                .mt_2()
                .text_sm()
                .text_color(rgb(0x7c7c7c))
                .text_center()
                .max_w(px(520.))
                .child(
                    "Open a texture to see it next to its upscaled version, or open a client jar to build a whole resource pack.",
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .mt_3()
                .child(primary_button("Open texture…", open_texture))
                .child(primary_button("Open jar…", open_jar)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .justify_center()
                .gap_3()
                .mt_4()
                .text_xs()
                .text_color(rgb(0x636363))
                .child("xBRZ keeps the pixel art crisp")
                .child("the faces of a block are stitched together")
                .child("animated textures stay animated")
                .child("the pack can be zipped and installed"),
        )
}

/// A button that stands out, for the things worth doing first.
fn primary_button(
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    div()
        .id(SharedString::from(format!("primary-{label}")))
        .px_3()
        .py_2()
        .rounded_sm()
        .bg(rgb(0x3c6ea8))
        .text_color(rgb(0xffffff))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x4a80c0)))
        .on_click(on_click)
        .child(label)
}

/// The comparison image: the original (blown up to the size of the result)
/// on the left of the split, the result on the right.
fn build_compare(source: &RgbaImage, result: &RgbaImage, split: f32) -> RgbaImage {
    let width = result.width();
    let height = result.height();
    let mut image = RgbaImage::new(width, height);
    let split_x = (width as f32 * split) as u32;
    for y in 0..height {
        for x in 0..width {
            let original = sample_nearest(source, x, y, width, height);
            let upscaled = *result.get_pixel(x, y);
            image.put_pixel(x, y, if x < split_x { original } else { upscaled });
        }
    }
    image
}

/// The zoom that makes a texture fill the canvas.
fn fit_zoom(width: u32, height: u32) -> f32 {
    // The size of the canvas without the bars around it.
    const CANVAS_WIDTH: f32 = 1100.;
    const CANVAS_HEIGHT: f32 = 520.;
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    (CANVAS_WIDTH / width)
        .min(CANVAS_HEIGHT / height)
        .clamp(1.0, 32.0)
}

/// A short note in the middle of the canvas.
fn hint_panel(text: &str) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(rgb(0x5f5f5f))
        .child(text.to_string())
}

fn stitch_toggle(
    stitch: bool,
    has_model: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let on = stitch && has_model;
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
        .border_color(if on { rgb(0x5b93d4) } else { rgb(0x3a3a3a) })
        .bg(if on { rgb(0x3c6ea8) } else { rgb(0x2e2e2e) })
        .text_color(if has_model {
            rgb(0xd6d6d6)
        } else {
            rgb(0x636363)
        })
        .child(check_box(on))
        .child("Stitch faces");

    if has_model {
        row.cursor_pointer()
            .hover(|style| style.bg(rgb(0x3a3a3a)))
            .on_click(on_click)
    } else {
        row
    }
}

/// A small square that is filled when the setting is on.
fn check_box(checked: bool) -> impl IntoElement {
    div()
        .w(px(12.))
        .h(px(12.))
        .border_1()
        .border_color(if checked {
            rgb(0xd6e6fa)
        } else {
            rgb(0x8a8a8a)
        })
        .bg(if checked {
            rgb(0xd6e6fa)
        } else {
            rgb(0x1a1a1a)
        })
        .rounded_sm()
}

fn to_render_image(rgba: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = rgba.clone();
    for pixel in bgra.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let frame = image::Frame::new(bgra);
    Arc::new(RenderImage::new(vec![frame]))
}
