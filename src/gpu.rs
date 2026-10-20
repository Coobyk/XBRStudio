//! The GPU backend.
//!
//! The edge adaptive upscaler runs as a compute shader, so a whole texture
//! is scaled in one dispatch. When no device is available the caller falls
//! back to the CPU implementation.

use image::RgbaImage;

const SHADER: &str = include_str!("gpu_shader.wgsl");

/// The parameters of one upscale.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Params {
    src_size: [u32; 2],
    dst_size: [u32; 2],
}

/// A device that can run the compute shader.
pub struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

impl GpuContext {
    /// Ask for a device. The error explains what went wrong when there is
    /// none.
    pub fn new() -> Result<GpuContext, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("no GPU adapter: {error}"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("xbrstudio"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::default(),
        }))
        .map_err(|error| format!("no GPU device: {error}"))?;

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xbrstudio upscale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xbrstudio upscale"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xbrstudio upscale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("xbrstudio upscale"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok(GpuContext {
            device,
            queue,
            pipeline,
            layout,
        })
    }

    /// Scale an image with the compute shader.
    pub fn upscale(&self, image: &RgbaImage, factor: u32) -> Result<RgbaImage, String> {
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err("cannot upscale an empty image".into());
        }
        let out_width = width * factor;
        let out_height = height * factor;

        let source_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("xbrstudio source"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let source_bytes = image.as_raw();
        let unpadded = width as usize * 4;
        // The rows of a texture have to be a multiple of 256 bytes.
        let stride = unpadded.div_ceil(256) * 256;
        let padded = {
            let mut buffer = vec![0u8; stride * height as usize];
            for row in 0..height as usize {
                let start = row * stride;
                buffer[start..start + unpadded]
                    .copy_from_slice(&source_bytes[row * unpadded..(row + 1) * unpadded]);
            }
            buffer
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &padded,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride as u32),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let params = Params {
            src_size: [width, height],
            dst_size: [out_width, out_height],
        };
        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xbrstudio params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut param_bytes = [0u8; 16];
        param_bytes[0..4].copy_from_slice(&params.src_size[0].to_le_bytes());
        param_bytes[4..8].copy_from_slice(&params.src_size[1].to_le_bytes());
        param_bytes[8..12].copy_from_slice(&params.dst_size[0].to_le_bytes());
        param_bytes[12..16].copy_from_slice(&params.dst_size[1].to_le_bytes());
        self.queue.write_buffer(&params_buffer, 0, &param_bytes);

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xbrstudio output"),
            size: out_width as u64 * out_height as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("xbrstudio upscale"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &source_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("xbrstudio upscale"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("xbrstudio upscale"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(out_width.div_ceil(8), out_height.div_ceil(8), 1);
            drop(pass);
        }

        let read_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xbrstudio readback"),
            size: out_width as u64 * out_height as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(&output_buffer, 0, &read_buffer, 0, read_buffer.size());

        self.queue.submit(Some(encoder.finish()));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("the GPU did not finish: {error}"))?;

        let slice = read_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("the GPU did not finish: {error}"))?;
        receiver
            .recv()
            .map_err(|_| "the GPU did not answer".to_string())?
            .map_err(|error| format!("cannot read the result: {error}"))?;

        let data = slice
            .get_mapped_range()
            .map_err(|error| format!("cannot read the result: {error}"))?;
        let mut result = RgbaImage::new(out_width, out_height);
        for y in 0..out_height {
            for x in 0..out_width {
                let offset = (y as usize * out_width as usize + x as usize) * 4;
                let pixel = image::Rgba([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]);
                result.put_pixel(x, y, pixel);
            }
        }
        drop(data);
        read_buffer.unmap();

        Ok(result)
    }
}

/// The device of the process, if one was asked for.
static CONTEXT: std::sync::OnceLock<Option<GpuContext>> = std::sync::OnceLock::new();

/// Look for a device and remember it.
///
/// Without a device the CPU implementation is used, so this never fails the
/// build; the error is only reported.
pub fn init() -> Result<(), String> {
    let context = GpuContext::new().ok();
    let found = context.is_some();
    let _ = CONTEXT.set(context);
    if found {
        Ok(())
    } else {
        Err("no GPU device found, the CPU implementation is used".into())
    }
}

/// The device of the process, when there is one.
pub fn context() -> Option<&'static GpuContext> {
    CONTEXT.get().and_then(|context| context.as_ref())
}

/// Scale an image with the GPU, falling back to the CPU when there is no
/// device.
pub fn upscale_or_fallback(
    image: &RgbaImage,
    factor: u32,
    context: Option<&GpuContext>,
) -> Result<RgbaImage, String> {
    match context {
        Some(context) => context.upscale(image, factor),
        None => Ok(crate::scalers::fsr::scale_fsr(image, factor)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn the_gpu_matches_the_cpu() {
        let Ok(context) = GpuContext::new() else {
            // No device in this environment, nothing to compare.
            return;
        };

        let mut image = RgbaImage::new(16, 16);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 16) as u8, (y * 16) as u8, 64, 255]);
        }

        let gpu = context.upscale(&image, 4).unwrap();
        let cpu = crate::scalers::fsr::scale_fsr(&image, 4);
        assert_eq!(gpu.dimensions(), cpu.dimensions());

        let mut difference = 0u64;
        for (a, b) in gpu.pixels().zip(cpu.pixels()) {
            for channel in 0..4 {
                difference += (a.0[channel] as i32 - b.0[channel] as i32).unsigned_abs() as u64;
            }
        }
        let average = difference as f64 / (gpu.width() as f64 * gpu.height() as f64 * 4.0);
        assert!(average < 2.0, "the GPU and the CPU differ by {average}");
    }
}
