#!/usr/bin/env python3
"""Fix the wgpu 30 API differences."""
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
            """        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());""",
            """        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::new_without_display_handle());""",
        ),
        (
            """        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|error| format!("no GPU adapter: {error}"))?;""",
            """        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("no GPU adapter: {error}"))?;""",
        ),
        (
            """        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("xbrstudio"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("no GPU device: {error}"))?;""",
            """        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("xbrstudio"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::default(),
        }))
        .map_err(|error| format!("no GPU device: {error}"))?;""",
        ),
        (
            """        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xbrstudio upscale"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });""",
            """        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xbrstudio upscale"),
            bind_group_layouts: Some(&layout),
            immediate_size: 0,
        });""",
        ),
        (
            """                bytes_per_row: Some(width as usize * 4),
                rows_per_image: Some(height as usize),""",
            """                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),""",
        ),
    ],
)
