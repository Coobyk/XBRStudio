#!/usr/bin/env python3
"""Wire the model picker into the jar flow and drop the unused drag code."""
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


# Fill the picker with the models of the jar that is being processed.
swap(
    """            let cancel = opts.cancel.clone();
            let factor = opts.factor;
            this.update(cx, |view, cx| {
                view.batch_cancel = cancel;
                view.batch_done = 0;
                view.batch_total = 0;
                view.batch_dir = Some(out_dir.clone());
                cx.notify();
            })
            .ok();""",
    """            let cancel = opts.cancel.clone();
            let factor = opts.factor;
            this.update(cx, |view, cx| {
                view.batch_cancel = cancel;
                view.batch_done = 0;
                view.batch_total = 0;
                view.batch_dir = Some(out_dir.clone());
                view.load_jar_models(jar.clone());
                cx.notify();
            })
            .ok();""",
)

# The drag handler is not used: the rotate buttons do the job.
swap(
    """    fn drag_3d(&mut self, position: (f32, f32), _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((last_x, last_y)) = self.last_drag {
            self.yaw += (position.0 - last_x) * 0.01;
            self.pitch = (self.pitch + (position.1 - last_y) * 0.01).clamp(-1.4, 1.4);
            self.update_preview();
            cx.notify();
        }
        self.last_drag = Some(position);
    }

""",
    "",
)

swap(
    """    last_drag: Option<(f32, f32)>,
""",
    "",
)

swap(
    """            last_drag: None,
""",
    "",
)

# The animation fields that are not used yet.
swap(
    """struct Animation {
    frame_width: u32,
    frame_height: u32,
    /// The time one frame is shown, in milliseconds.
    frame_time: u64,
}""",
    """struct Animation {
    #[allow(dead_code)]
    frame_width: u32,
    frame_height: u32,
    /// The time one frame is shown, in milliseconds.
    #[allow(dead_code)]
    frame_time: u64,
}""",
)

open(p, "w").write(s)
print("gui cleaned up")
