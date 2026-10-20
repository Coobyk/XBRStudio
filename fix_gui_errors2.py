#!/usr/bin/env python3
"""Fix the remaining GUI errors."""
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


swap(
    """                let y = frame * animation.frame_height;""",
    """                let y = frame as u32 * animation.frame_height;""",
)

swap(
    """        let model_listeners: Vec<_> = model_names
            .into_iter()
            .map(|name| {
                let listener = cx.listener(move |view, event, window, cx| {
                    view.pick_model(name.clone(), event, window, cx);
                });
                (name, listener)
            })
            .collect();""",
    """        let model_listeners: Vec<_> = model_names
            .iter()
            .map(|name| {
                let owned = name.clone();
                let listener = cx.listener(move |view, event, window, cx| {
                    view.pick_model(owned.clone(), event, window, cx);
                });
                (name.clone(), listener)
            })
            .collect();""",
)

swap(
    """                                                .children(rotate_listeners.into_iter().map(
                                                    |(yaw, listener)| {
                                                        let label = match (yaw, listener) {
                                                            _ => "",
                                                        };
                                                        let _ = label;
                                                        toolbar_button("rotate", true, listener)
                                                    },
                                                )),""",
    """                                                .children(rotate_listeners.into_iter().map(
                                                    |(_yaw, listener)| {
                                                        toolbar_button("rotate", true, listener)
                                                    },
                                                )),""",
)

swap(
    """        .child(box_el)
        .child("Stitch faces");""",
    """        .child(box_el())
        .child("Stitch faces");""",
)

open(p, "w").write(s)
print("gui fixed")
