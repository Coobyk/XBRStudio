#!/usr/bin/env python3
"""Add pollster, register the gpu module and use it for the FSR path."""
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
    "Cargo.toml",
    [
        (
            """wgpu = { version = "30", optional = true }""",
            """wgpu = { version = "30", optional = true }
pollster = { version = "0.4", optional = true }""",
        ),
        (
            """gpu = ["dep:wgpu"]""",
            """gpu = ["dep:wgpu", "dep:pollster"]""",
        ),
    ],
)

patch(
    "src/lib.rs",
    [
        (
            """pub mod jar;""",
            """#[cfg(feature = "gpu")]
pub mod gpu;
pub mod jar;""",
        ),
    ],
)

patch(
    "src/gpu.rs",
    [
        (
            """        self.queue
            .write_buffer(&params_buffer, 0, bytemuck::bytes_of(&params));""",
            """        let mut param_bytes = [0u8; 16];
        param_bytes[0..4].copy_from_slice(&params.src_size[0].to_le_bytes());
        param_bytes[4..8].copy_from_slice(&params.src_size[1].to_le_bytes());
        param_bytes[8..12].copy_from_slice(&params.dst_size[0].to_le_bytes());
        param_bytes[12..16].copy_from_slice(&params.dst_size[1].to_le_bytes());
        self.queue.write_buffer(&params_buffer, 0, &param_bytes);""",
        ),
        (
            """/// Scale an image with the GPU, falling back to the CPU when there is no
/// device.""",
            """/// The device of the process, if one was asked for.
static CONTEXT: std::sync::OnceLock<Option<GpuContext>> = std::sync::OnceLock::new();

/// Look for a device and remember it.
///
/// Without a device the CPU implementation is used, so this never fails the
/// build; the error is only reported.
pub fn init() -> Result<(), String> {
    let context = GpuContext::new();
    let _ = CONTEXT.set(context);
    match CONTEXT.get().and_then(|context| context.as_ref()) {
        Some(_) => Ok(()),
        None => Err("no GPU device found, the CPU implementation is used".into()),
    }
}

/// The device of the process, when there is one.
pub fn context() -> Option<&'static GpuContext> {
    CONTEXT.get().and_then(|context| context.as_ref())
}

/// Scale an image with the GPU, falling back to the CPU when there is no
/// device.""",
        ),
    ],
)

patch(
    "src/upscaling.rs",
    [
        (
            """        Algorithm::Fsr => crate::scalers::fsr::scale_fsr(&source, config.factor),""",
            """        Algorithm::Fsr => {
            // The GPU backend runs the same algorithm, so the result is the
            // same up to the rounding.
            #[cfg(feature = "gpu")]
            if let Some(context) = crate::gpu::context() {
                return context.upscale(&source, config.factor);
            }
            crate::scalers::fsr::scale_fsr(&source, config.factor)
        }""",
        ),
    ],
)
