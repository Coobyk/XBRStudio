#!/usr/bin/env python3
"""Fix the poll type and the context handling."""
import sys

p = "src/gpu.rs"
s = open(p).read()

s = s.replace(
    ".poll(wgpu::PollType::wait())",
    ".poll(wgpu::PollType::wait_indefinitely())",
)

old = """pub fn init() -> Result<(), String> {
    let context = GpuContext::new();
    let _ = CONTEXT.set(context);
    match CONTEXT.get().and_then(|context| context.as_ref()) {
        Some(_) => Ok(()),
        None => Err("no GPU device found, the CPU implementation is used".into()),
    }
}"""
new = """pub fn init() -> Result<(), String> {
    let context = GpuContext::new().ok();
    let found = context.is_some();
    let _ = CONTEXT.set(context);
    if found {
        Ok(())
    } else {
        Err("no GPU device found, the CPU implementation is used".into())
    }
}"""
assert old in s
s = s.replace(old, new)

old = """    match context {
        Some(context) => context.upscale(image, factor),
        None => crate::scalers::fsr::scale_fsr(image, factor),
    }
}"""
new = """    match context {
        Some(context) => context.upscale(image, factor),
        None => Ok(crate::scalers::fsr::scale_fsr(image, factor)),
    }
}"""
assert old in s
s = s.replace(old, new)

open(p, "w").write(s)
print("patched src/gpu.rs")
