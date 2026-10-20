#!/usr/bin/env python3
"""Fix the GUI: use cx.listener and add the model faces field."""
import sys

p = "src/bin/gui.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:400])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:400]))
        sys.exit(1)
    s = s.replace(old, new)


# The model faces field.
swap(
    """struct AppView {
    texture_path: Option<PathBuf>,
    model_label: String,""",
    """struct AppView {
    texture_path: Option<PathBuf>,
    model_label: String,
    model_faces: Option<ModelFaces>,""",
)

swap(
    """        Self {
            texture_path: None,
            model_label: "—".into(),""",
    """        Self {
            texture_path: None,
            model_label: "—".into(),
            model_faces: None,""",
)

# The toolbar button helper takes a listener.
swap(
    """fn toolbar_button(
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&mut AppView, &ClickEvent, &mut Window, &mut Context<AppView>) + 'static,
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
            .on_click(move |click, window, cx| {
                cx.update(|view: &mut AppView, cx| on_click(view, click, window, cx));
            })
    } else {
        button.text_color(rgb(0x6a6a6a))
    }
}""",
    """fn toolbar_button(
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
}""",
)

swap(
    """fn stitch_toggle(
    enabled: bool,
    has_model: bool,
    on_click: impl Fn(&mut AppView, &ClickEvent, &mut Window, &mut Context<AppView>) + 'static,
) -> impl IntoElement {""",
    """fn stitch_toggle(
    enabled: bool,
    has_model: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {""",
)

swap(
    """    if enabled && has_model {
        row.cursor_pointer()
            .hover(|style| style.bg(rgb(0x4a4a4a)))
            .on_click(move |click, window, cx| {
                cx.update(|view: &mut AppView, cx| on_click(view, click, window, cx));
            })
    } else {
        row.text_color(rgb(0x6a6a6a))
    }
}""",
    """    if enabled && has_model {
        row.cursor_pointer()
            .hover(|style| style.bg(rgb(0x4a4a4a)))
            .on_click(on_click)
    } else {
        row.text_color(rgb(0x6a6a6a))
    }
}""",
)

# The toolbar: build the listeners first.
swap(
    """        let frame = self.frame;
        let frames = self
            .animation
            .map(|animation| frame_count(animation, self.source.as_ref().map(|i| i.height()).unwrap_or(1)))
            .unwrap_or(1);
""",
    """        let frame = self.frame;
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
            .into_iter()
            .map(|name| {
                let listener = cx.listener(move |view, event, window, cx| {
                    view.pick_model(name.clone(), event, window, cx);
                });
                (name, listener)
            })
            .collect();
""",
)

# Replace the toolbar buttons with the listeners.
swap(
    """                    .child(toolbar_button("Open texture…", true, Self::open_texture))
                    .child(toolbar_button("Open model…", true, Self::open_model))
                    .child(toolbar_button("Open jar…", !batch_running, Self::open_jar))
                    .child(toolbar_button(
                        if picker_open { "Hide models" } else { "Models…" },
                        true,
                        Self::toggle_picker,
                    ))
                    .child(div().w(px(8.)))
                    .child(toolbar_button("2×", factor != 2, |view, event, window, cx| {
                        view.set_factor(2, event, window, cx)
                    }))
                    .child(toolbar_button("3×", factor != 3, |view, event, window, cx| {
                        view.set_factor(3, event, window, cx)
                    }))
                    .child(toolbar_button("4×", factor != 4, |view, event, window, cx| {
                        view.set_factor(4, event, window, cx)
                    }))
                    .child(toolbar_button("5×", factor != 5, |view, event, window, cx| {
                        view.set_factor(5, event, window, cx)
                    }))
                    .child(toolbar_button("6×", factor != 6, |view, event, window, cx| {
                        view.set_factor(6, event, window, cx)
                    }))
                    .child(div().w(px(8.)))
                    .child(toolbar_button("xbrz", algorithm != Algorithm::Xbrz, |view, event, window, cx| {
                        view.set_algorithm(Algorithm::Xbrz, event, window, cx)
                    }))
                    .child(toolbar_button("fsr", algorithm != Algorithm::Fsr, |view, event, window, cx| {
                        view.set_algorithm(Algorithm::Fsr, event, window, cx)
                    }))
                    .child(toolbar_button("lanczos", algorithm != Algorithm::Lanczos3, |view, event, window, cx| {
                        view.set_algorithm(Algorithm::Lanczos3, event, window, cx)
                    }))
                    .child(toolbar_button("bicubic", algorithm != Algorithm::Bicubic, |view, event, window, cx| {
                        view.set_algorithm(Algorithm::Bicubic, event, window, cx)
                    }))
                    .child(div().w(px(8.)))
                    .child(toolbar_button("alpha: on", alpha != AlphaMode::Masked, |view, event, window, cx| {
                        view.set_alpha(AlphaMode::Masked, event, window, cx)
                    }))
                    .child(toolbar_button("sharpen", sharpen <= 0.0, |view, event, window, cx| {
                        view.set_sharpen(0.25, event, window, cx)
                    }))
                    .child(div().w(px(8.)))
                    .child(stitch_toggle(has_model && stitch, has_model, Self::toggle_stitch))
                    .child(toolbar_button("Upscale", has_source, Self::run_upscale))
                    .child(toolbar_button("Save…", has_result, Self::save_result))
                    .child(toolbar_button("Build pack", true, Self::build_pack))
                    .child(toolbar_button("Install", true, Self::install_pack))
                    .child(toolbar_button(
                        if show_3d { "Hide 3D" } else { "3D preview" },
                        has_model,
                        Self::toggle_3d,
                    )),""",
    """                    .child(toolbar_button("Open texture…", true, open_texture))
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
                    )),""",
)

# The model list.
swap(
    """                                .children(model_names.into_iter().map(|name| {
                                    let label = name.clone();
                                    div()
                                        .id(SharedString::from(format!("model-{name}")))
                                        .px_2()
                                        .py_1()
                                        .cursor_pointer()
                                        .text_color(rgb(0xd0d0d0))
                                        .hover(|style| style.bg(rgb(0x333333)))
                                        .on_click(move |click, window, cx| {
                                            cx.update(|view: &mut AppView, cx| {
                                                view.pick_model(label.clone(), click, window, cx);
                                            })
                                        })
                                        .child(name)
                                })),""",
    """                                .children(model_listeners.into_iter().map(|(name, listener)| {
                                    div()
                                        .id(SharedString::from(format!("model-{name}")))
                                        .px_2()
                                        .py_1()
                                        .cursor_pointer()
                                        .text_color(rgb(0xd0d0d0))
                                        .hover(|style| style.bg(rgb(0x333333)))
                                        .on_click(listener)
                                        .child(name)
                                })),""",
)

# The comparison slider.
swap(
    """                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .gap_1()
                                                    .children((0..=20).map(|step| {
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
                                                            .on_click(move |click, window, cx| {
                                                                cx.update(|view: &mut AppView, cx| {
                                                                    view.set_split(position, click, window, cx);
                                                                })
                                                            })
                                                    })),
                                            )""",
    """                                            .child(
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
                                            )""",
)

# The 3D rotation buttons.
swap(
    """                                                .child(toolbar_button("← rotate", true, |view, event, window, cx| {
                                                    view.rotate_3d(-0.4, 0.0, event, window, cx)
                                                }))
                                                .child(toolbar_button("rotate →", true, |view, event, window, cx| {
                                                    view.rotate_3d(0.4, 0.0, event, window, cx)
                                                }))
                                                .child(toolbar_button("↑ tilt", true, |view, event, window, cx| {
                                                    view.rotate_3d(0.0, 0.3, event, window, cx)
                                                }))
                                                .child(toolbar_button("↓ tilt", true, |view, event, window, cx| {
                                                    view.rotate_3d(0.0, -0.3, event, window, cx)
                                                })),""",
    """                                                .children(rotate_listeners.into_iter().map(
                                                    |(yaw, listener)| {
                                                        let label = match (yaw, listener) {
                                                            _ => "",
                                                        };
                                                        let _ = label;
                                                        toolbar_button("rotate", true, listener)
                                                    },
                                                )),""",
)

swap(
    """                    .when(batch_running, |this| {
                        this.child(toolbar_button("Cancel", true, Self::cancel_batch))
                    }),""",
    """                    .when(batch_running, |this| {
                        this.child(toolbar_button("Cancel", true, cancel_batch))
                    }),""",
)

open(p, "w").write(s)
print("gui.rs listeners fixed")
