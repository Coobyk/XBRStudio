#!/usr/bin/env python3
"""Fix the remaining GUI compile errors."""
import sys


def patch(path, pairs):
    with open(path, "r", encoding="utf-8") as handle:
        source = handle.read()
    for old, new in pairs:
        if old not in source:
            print("MISSING in %s:\n%s" % (path, old[:300]))
            sys.exit(1)
        if source.count(old) != 1:
            print("NOT UNIQUE in %s (%d):\n%s" % (path, source.count(old), old[:300]))
            sys.exit(1)
        source = source.replace(old, new)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(source)
    print("patched", path)


patch(
    "src/bin/gui.rs",
    [
        (
            """                                                "{texture_label}    {model_label}    faces: {face_count}    frame: {}/{frames}",
                                                frame + 1,
                                                frames""",
            """                                                "{texture_label}    {model_label}    faces: {face_count}    frame: {}/{}",
                                                frame + 1,
                                                frames""",
        ),
        (
            """                            div()
                                .flex()
                                .flex_col()
                                .w(px(220.))
                                .border_r_1()
                                .border_color(rgb(0x3c3c3c))
                                .overflow_y_scroll()""",
            """                            div()
                                .id("model-picker")
                                .flex()
                                .flex_col()
                                .w(px(220.))
                                .border_r_1()
                                .border_color(rgb(0x3c3c3c))
                                .overflow_y_scroll()""",
        ),
        (
            """                                            .child(toolbar_button("−", true, Self::zoom_out))
                                            .child(toolbar_button("+", true, Self::zoom_in)),""",
            """                                            .child(toolbar_button("−", true, zoom_out))
                                            .child(toolbar_button("+", true, zoom_in)),""",
        ),
    ],
)

patch(
    "src/jar.rs",
    [
        (
            """fn animation_frame_size(width: u32, height: u32, animation: &Value) -> (u32, u32) {""",
            """pub fn animation_frame_size(width: u32, height: u32, animation: &Value) -> (u32, u32) {""",
        ),
    ],
)
