#!/usr/bin/env python3
"""Fix the remaining wgpu 30 API differences."""
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
    "src/gpu.rs",
    [
        (
            """        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::new_without_display_handle());""",
            """        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());""",
        ),
        (
            """            bind_group_layouts: Some(&layout),""",
            """            bind_group_layouts: &[Some(&layout)],""",
        ),
        (
            """            pass.dispatch_workgroups(
                out_width.div_ceil(8),
                out_height.div_ceil(8),
                1,
            );
            pass.end();""",
            """            pass.dispatch_workgroups(out_width.div_ceil(8), out_height.div_ceil(8), 1);
            drop(pass);""",
        ),
        (
            """        self.device
            .poll(wgpu::PollType::Wait)
            .map_err(|error| format!("the GPU did not finish: {error}"))?;

        let slice = read_buffer.slice(..);""",
            """        self.device
            .poll(wgpu::PollType::wait())
            .map_err(|error| format!("the GPU did not finish: {error}"))?;

        let slice = read_buffer.slice(..);""",
        ),
        (
            """        self.device
            .poll(wgpu::PollType::Wait)
            .map_err(|error| format!("the GPU did not finish: {error}"))?;
        receiver""",
            """        self.device
            .poll(wgpu::PollType::wait())
            .map_err(|error| format!("the GPU did not finish: {error}"))?;
        receiver""",
        ),
        (
            """        let data = slice.get_mapped_range();""",
            """        let data = slice
            .get_mapped_range()
            .map_err(|error| format!("cannot read the result: {error}"))?;""",
        ),
    ],
)
