use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, ClickEvent, Context, PathPromptOptions, RenderImage, SharedString,
    TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div, img, prelude::*, px, rgb,
    size,
};
use image::RgbaImage;

use xbrstudio::jar::{BatchOptions, BatchProgress, JarMessage, is_wrap_texture_path, spawn_batch};
use xbrstudio::model::{ModelFaces, load_model};
use xbrstudio::upscaling::{UpscaleConfig, upscale_image, upscale_wrapped};

actions!(xbrstudio, [Quit]);

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-q", Quit, None),
            gpui::KeyBinding::new("ctrl-q", Quit, None),
        ]);

        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
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
    model_path: Option<PathBuf>,
    source: Option<RgbaImage>,
    model_faces: Option<ModelFaces>,
    result: Option<RgbaImage>,
    source_image: Option<Arc<RenderImage>>,
    result_image: Option<Arc<RenderImage>>,
    factor: u32,
    stitch: bool,
    zoom: f32,
    status: String,
    error: Option<String>,
    batch_running: bool,
}

impl AppView {
    fn new() -> Self {
        Self {
            texture_path: None,
            model_path: None,
            source: None,
            model_faces: None,
            result: None,
            source_image: None,
            result_image: None,
            factor: 4,
            stitch: true,
            zoom: 4.0,
            status: "Open a texture to begin.".into(),
            error: None,
            batch_running: false,
        }
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    fn clear_result(&mut self) {
        self.result = None;
        self.result_image = None;
    }

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
                }
            }) {
                Ok(opts) => opts,
                Err(_) => return,
            };
            let factor = opts.factor;
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
                                view.status = format!("[{done}/{total}] {name}");
                                cx.notify();
                            })
                            .ok();
                        }
                        Ok(JarMessage::Finished(result)) => {
                            this.update(cx, |view, cx| {
                                view.batch_running = false;
                                match result {
                                    Ok(report) => {
                                        if report.upscaled == 0
                                            && !report.errors.is_empty()
                                        {
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
                                            if report.wrapped > 0 {
                                                summary.push_str(&format!(
                                                    ", {} block-wrapped",
                                                    report.wrapped
                                                ));
                                            }
                                            if report.animated > 0 {
                                                summary.push_str(&format!(
                                                    ", {} animated",
                                                    report.animated
                                                ));
                                            }
                                            if report.stitched > 0 {
                                                summary.push_str(&format!(
                                                    ", {} stitched via {} entity + {} block models",
                                                    report.stitched,
                                                    report.entity_models,
                                                    report.block_models
                                                ));
                                            }
                                            if !report.unmatched_entity.is_empty() {
                                                summary.push_str(&format!(
                                                    ", {} entity textures unmatched",
                                                    report.unmatched_entity.len()
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

    fn clear_model(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.model_faces = None;
        self.model_path = None;
        self.clear_result();
        self.status = "Model cleared.".into();
        self.error = None;
        cx.notify();
    }

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
        };

        let result = if wrap {
            upscale_wrapped(&source, self.factor)
        } else {
            upscale_image(&source, faces.as_deref(), &config)
        };
        match result {
            Ok(result) => {
                let group_note = if stitch && faces.is_some() {
                    format!(
                        "; stitched {} faces across {} boxes",
                        faces.as_ref().map(Vec::len).unwrap_or(0),
                        xbrstudio::upscaling::box_count(faces.as_deref().unwrap_or(&[]))
                    )
                } else if wrap {
                    "; tiled wrap".to_string()
                } else {
                    String::new()
                };

                self.result_image = Some(to_render_image(&result));
                self.result = Some(result);
                let result_ref = self.result.as_ref().expect("just set");
                self.status = format!(
                    "Upscaled {}x{} -> {}x{} (factor {}{})",
                    source.width(),
                    source.height(),
                    result_ref.width(),
                    result_ref.height(),
                    self.factor,
                    group_note
                );
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
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

    fn load_texture(&mut self, path: PathBuf) {
        match image::open(&path) {
            Ok(image) => {
                let rgba = image.to_rgba8();
                self.source_image = Some(to_render_image(&rgba));
                self.source = Some(rgba);
                self.texture_path = Some(path);
                self.clear_result();
                self.error = None;
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
                self.rescale_model_faces();
            }
            Err(error) => self.set_error(format!("cannot read texture: {error}")),
        }
    }

    fn load_model_file(&mut self, path: PathBuf) {
        match load_model(&path) {
            Ok(mut faces) => {
                if let Some(source) = &self.source {
                    xbrstudio::model::scale_model_faces_to_image(&mut faces, source);
                } else {
                    self.model_path = Some(path);
                    self.model_faces = Some(faces);
                    self.stitch = true;
                    self.clear_result();
                    self.status = "Model queued — open a texture to map UV faces.".into();
                    self.error = None;
                    return;
                }

                if faces.faces.is_empty() {
                    self.set_error(format!(
                        "model {} has no face UV rectangles inside the texture",
                        path.display()
                    ));
                    return;
                }

                self.status = format!(
                    "Loaded model {} ({} faces).",
                    path.display(),
                    faces.faces.len()
                );
                self.model_faces = Some(faces);
                self.model_path = Some(path);
                self.stitch = true;
                self.clear_result();
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn rescale_model_faces(&mut self) {
        let (Some(source), Some(path)) = (&self.source, self.model_path.clone()) else {
            return;
        };
        let source = source.clone();
        match load_model(&path) {
            Ok(mut faces) => {
                xbrstudio::model::scale_model_faces_to_image(&mut faces, &source);
                if faces.faces.is_empty() {
                    self.model_faces = None;
                    self.model_path = None;
                    self.set_error(format!(
                        "model {} has no face UV rectangles inside the texture",
                        path.display()
                    ));
                    return;
                }
                self.model_faces = Some(faces);
                self.status = format!(
                    "Loaded model {} ({} faces).",
                    path.display(),
                    self.model_faces
                        .as_ref()
                        .map(|m| m.faces.len())
                        .unwrap_or(0)
                );
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_source = self.source.is_some();
        let has_result = self.result.is_some();
        let has_model = self.model_faces.is_some();
        let stitch = self.stitch;
        let factor = self.factor;
        let batch_running = self.batch_running;
        let status_line = match &self.error {
            Some(error) => format!("error: {error}"),
            None => self.status.clone(),
        };
        let texture_label = self
            .texture_path
            .as_deref()
            .map(|path| format!("texture: {}", path.display()))
            .unwrap_or_else(|| "texture: —".into());
        let model_label = self
            .model_path
            .as_deref()
            .map(|path| format!("model: {}", path.display()))
            .unwrap_or_else(|| "model: —".into());
        let face_count = self
            .model_faces
            .as_ref()
            .map(|m| m.faces.len())
            .unwrap_or(0);
        let zoom = self.zoom;
        let source_image = self.source_image.clone();
        let result_image = self.result_image.clone();
        let source_size = self
            .source
            .as_ref()
            .map(|image| (image.width(), image.height()));
        let result_size = self
            .result
            .as_ref()
            .map(|image| (image.width(), image.height()));
        let open_texture = cx.listener(Self::open_texture);
        let open_jar = cx.listener(Self::open_jar);
        let open_model = cx.listener(Self::open_model);
        let clear_model = cx.listener(Self::clear_model);
        let run_upscale = cx.listener(Self::run_upscale);
        let save_result = cx.listener(Self::save_result);
        let toggle_stitch = cx.listener(Self::toggle_stitch);
        let zoom_in = cx.listener(Self::zoom_in);
        let zoom_out = cx.listener(Self::zoom_out);
        let factor_listeners: Vec<_> = [2u32, 3, 4, 5, 6]
            .into_iter()
            .map(|value| {
                let listener = cx.listener(move |view, event, window, cx| {
                    view.set_factor(value, event, window, cx);
                });
                (value, listener)
            })
            .collect();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xd4d4d4))
            .text_sm()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .bg(rgb(0x2d2d2d))
                    .border_b_1()
                    .border_color(rgb(0x404040))
                    .child(toolbar_button("Open texture…", true, open_texture))
                    .child(toolbar_button("Open jar…", !batch_running, open_jar))
                    .child(toolbar_button(
                        if self.model_path.is_some() {
                            "Change model…"
                        } else {
                            "Load model…"
                        },
                        true,
                        open_model,
                    ))
                    .child(toolbar_button(
                        "Clear model",
                        has_model,
                        clear_model,
                    ))
                    .child(div().id("sep1").w_1().h_4().bg(rgb(0x404040)).into_any_element())
                    .child(div().child("Factor").into_any_element())
                    .children(factor_listeners.into_iter().map(|(value, listener)| {
                        let selected = factor == value;
                        factor_chip(value, selected, listener)
                    }))
                    .child(stitch_toggle(has_model, stitch, toggle_stitch))
                    .child(div().id("sep2").w_1().h_4().bg(rgb(0x404040)).into_any_element())
                    .child(toolbar_button("Upscale", has_source, run_upscale))
                    .child(toolbar_button("Save…", has_result, save_result))
                    .child(div().id("sep3").w_1().h_4().bg(rgb(0x404040)).into_any_element())
                    .child(div().child(format!("Zoom {:.1}×", zoom)).into_any_element())
                    .child(toolbar_button("−", true, zoom_out))
                    .child(toolbar_button("+", true, zoom_in)),
            )
            .child({
                let content = if source_image.is_none() {
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child(div().text_xl().child("XBR Studio"))
                        .child(
                            div()
                                .text_color(rgb(0x9a9a9a))
                                .child("Upscale Minecraft textures with xBRZ."),
                        )
                        .child(
                            div()
                                .text_color(rgb(0x9a9a9a))
                                .child("Optionally load a model JSON to stitch neighboring faces."),
                        )
                        .child(
                            div()
                                .text_color(rgb(0x9a9a9a))
                                .child("Or open a Minecraft jar to upscale every texture into a resource pack."),
                        )
                        .into_any_element()
                } else {
                    div()
                        .size_full()
                        .flex()
                        .flex_row()
                        .gap_4()
                        .p_4()
                        .child(
                            preview_panel(
                                "Original",
                                source_size,
                                source_image,
                                zoom,
                                "original",
                            ),
                        )
                        .when(has_result, |this| {
                            this.child(preview_panel(
                                "Result",
                                result_size,
                                result_image,
                                zoom,
                                "result",
                            ))
                        })
                        .into_any_element()
                };
                content
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .bg(rgb(0x2d2d2d))
                    .border_t_1()
                    .border_color(rgb(0x404040))
                    .child(
                        div()
                            .text_color(if self.error.is_some() {
                                rgb(0xf06464)
                            } else {
                                rgb(0xd4d4d4)
                            })
                            .child(status_line),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x9a9a9a))
                            .text_xs()
                            .child(format!("{texture_label}    {model_label}    faces: {face_count}")),
                    ),
            )
    }
}

fn toolbar_button(
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    let base = div()
        .id(SharedString::from(format!("btn-{label}")))
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .cursor_pointer();

    if enabled {
        base.bg(rgb(0x3c3c3c))
            .border_color(rgb(0x5a5a5a))
            .hover(|style| style.bg(rgb(0x4a4a4a)))
            .active(|style| style.bg(rgb(0x2a2a2a)))
            .child(label)
            .on_click(on_click)
    } else {
        base.bg(rgb(0x2a2a2a))
            .border_color(rgb(0x3a3a3a))
            .text_color(rgb(0x6a6a6a))
            .cursor_default()
            .child(label)
    }
}

fn factor_chip(
    value: u32,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let base = div()
        .id(SharedString::from(format!("factor-{value}")))
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .cursor_pointer()
        .child(value.to_string());

    if selected {
        base.bg(rgb(0x264f78))
            .border_color(rgb(0x3d7ab0))
            .text_color(rgb(0xffffff))
            .on_click(on_click)
    } else {
        base.bg(rgb(0x3c3c3c))
            .border_color(rgb(0x5a5a5a))
            .hover(|style| style.bg(rgb(0x4a4a4a)))
            .on_click(on_click)
    }
}

fn stitch_toggle(
    enabled: bool,
    checked: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let box_el = div()
        .size(px(14.))
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x5a5a5a))
        .when(checked, |el| {
            el.bg(rgb(0x264f78)).border_color(rgb(0x3d7ab0))
        });

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
        .child(box_el)
        .child("Stitch faces");

    if enabled {
        row.cursor_pointer()
            .hover(|style| style.bg(rgb(0x4a4a4a)))
            .on_click(on_click)
    } else {
        row.text_color(rgb(0x6a6a6a))
    }
}

fn preview_panel(
    title: &str,
    dims: Option<(u32, u32)>,
    image: Option<Arc<RenderImage>>,
    zoom: f32,
    id: &str,
) -> impl IntoElement {
    let size_label = match dims {
        Some((width, height)) => format!("{title}  {width}x{height}"),
        None => title.to_string(),
    };

    div()
        .id(SharedString::from(format!("preview-{id}")))
        .flex()
        .flex_col()
        .gap_2()
        .min_w(px(0.))
        .w_full()
        .h_full()
        .child(div().text_color(rgb(0xb0b0b0)).child(size_label))
        .child(
            div()
                .size_full()
                .min_h(px(120.))
                .bg(rgb(0x141414))
                .border_1()
                .border_color(rgb(0x404040))
                .rounded_sm()
                .p_2()
                .overflow_hidden()
                .when_some(image, |this, image| {
                    this.child(
                        img(image)
                            .id(SharedString::from(format!("img-{id}")))
                            .object_fit(gpui::ObjectFit::Contain)
                            .w(px(16. * zoom.max(1.0)))
                            .h(px(16. * zoom.max(1.0))),
                    )
                }),
        )
}

fn to_render_image(rgba: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = rgba.clone();
    for pixel in (&mut *bgra).chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let frame = image::Frame::new(bgra);
    Arc::new(RenderImage::new(vec![frame]))
}
