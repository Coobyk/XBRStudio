//! The GPU backend.
//!
//! Two algorithms run on the GPU: the edge adaptive upscaler (FSR style) and
//! xBRZ. The xBRZ shader is a transliteration of the reference algorithm, so
//! its output is the same as the CPU one, pixel for pixel.
//!
//! When no device is available the caller falls back to the CPU
//! implementation.

use image::RgbaImage;

const SHADER: &str = include_str!("gpu_shader.wgsl");
const XBRZ_SHADER: &str = include_str!("gpu_xbrz.wgsl");

/// The parameters of one edge adaptive upscale.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Params {
    src_size: [u32; 2],
    dst_size: [u32; 2],
}

/// The colour distance table of the xBRZ algorithm.
///
/// The colour differences are halved and cut to five bits — the sign lives in
/// the top bit — so the table has 32³ entries. It is built exactly like the
/// one of the reference implementation, otherwise the blend decisions (and
/// therefore the pixels) would differ.
fn xbrz_lut() -> &'static [f32] {
    static LUT: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        const K_B: f64 = 0.0593;
        const K_R: f64 = 0.2627;
        const K_G: f64 = 1.0 - K_B - K_R;
        const SCALE_B: f64 = 0.5 / (1.0 - K_B);
        const SCALE_R: f64 = 0.5 / (1.0 - K_R);

        fn distance(r: f64, g: f64, b: f64) -> f64 {
            let y = K_R * r + K_G * g + K_B * b;
            let c_b = SCALE_B * (b - y);
            let c_r = SCALE_R * (r - y);
            (y * y + c_b * c_b + c_r * c_r).sqrt()
        }

        let mut table = Vec::with_capacity(0x8000);
        for index in 0..0x8000usize {
            let r = ((((index >> 10) & 0x1F) << 3) as u8) as i8 as i16 * 2;
            let g = ((((index >> 5) & 0x1F) << 3) as u8) as i8 as i16 * 2;
            let b = (((index & 0x1F) << 3) as u8) as i8 as i16 * 2;
            table.push(distance(r as f64, g as f64, b as f64) as f32);
        }
        table
    })
}

/// A device that can run the compute shaders.
pub struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    xbrz_pipeline: wgpu::ComputePipeline,
    xbrz_layout: wgpu::BindGroupLayout,
    lut_buffer: wgpu::Buffer,
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

        let (layout, pipeline) =
            Self::build_pipeline(&device, SHADER, "xbrstudio upscale", Vec::new())?;

        // The xBRZ shader needs the colour distance table as well.
        let (xbrz_layout, xbrz_pipeline) = Self::build_pipeline(
            &device,
            XBRZ_SHADER,
            "xbrstudio xbrz",
            vec![wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        )?;

        let lut = xbrz_lut();
        let lut_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xbrstudio colour distance"),
            size: (lut.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&lut_buffer, 0, bytemuck::cast_slice(lut));

        Ok(GpuContext {
            device,
            queue,
            pipeline,
            layout,
            xbrz_pipeline,
            xbrz_layout,
            lut_buffer,
        })
    }

    /// Build a compute pipeline and its bind group layout.
    ///
    /// The first two bindings are always the source texture and the uniform
    /// parameters, the third one the output buffer; the extra entries are
    /// added after those.
    fn build_pipeline(
        device: &wgpu::Device,
        source: &str,
        label: &str,
        extra: Vec<wgpu::BindGroupLayoutEntry>,
    ) -> Result<(wgpu::BindGroupLayout, wgpu::ComputePipeline), String> {
        let mut entries = vec![
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
        ];
        entries.extend(extra);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &entries,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok((layout, pipeline))
    }

    /// Put an image into a texture.
    fn upload_texture(&self, image: &RgbaImage) -> wgpu::Texture {
        let (width, height) = image.dimensions();
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
                texture: &texture,
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

        texture
    }

    /// Read a buffer of pixels back into an image.
    fn read_pixels(
        &self,
        buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, String> {
        let read_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xbrstudio readback"),
            size: buffer.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("xbrstudio readback"),
            });
        encoder.copy_buffer_to_buffer(buffer, 0, &read_buffer, 0, read_buffer.size());
        self.queue.submit(Some(encoder.finish()));

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
        let mut image = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let offset = (y as usize * width as usize + x as usize) * 4;
                image.put_pixel(
                    x,
                    y,
                    image::Rgba([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]),
                );
            }
        }
        drop(data);
        read_buffer.unmap();
        Ok(image)
    }

    /// Scale an image with the edge adaptive compute shader.
    pub fn upscale(&self, image: &RgbaImage, factor: u32) -> Result<RgbaImage, String> {
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err("cannot upscale an empty image".into());
        }
        let out_width = width * factor;
        let out_height = height * factor;

        let source_texture = self.upload_texture(image);

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
        self.queue.submit(Some(encoder.finish()));

        self.read_pixels(&output_buffer, out_width, out_height)
    }

    /// Scale an image with the xBRZ compute shader.
    ///
    /// The result is the same as the CPU implementation, pixel for pixel.
    pub fn xbrz(&self, image: &RgbaImage, factor: u32) -> Result<RgbaImage, String> {
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err("cannot upscale an empty image".into());
        }
        if !(2..=6).contains(&factor) {
            return Err(format!("xBRZ supports a factor of 2 to 6, not {factor}"));
        }
        let out_width = width * factor;
        let out_height = height * factor;

        let source_texture = self.upload_texture(image);

        // The uniform address space wants the size to be a multiple of
        // sixteen, so the buffer is larger than the parameters.
        let mut param_bytes = [0u8; 32];
        param_bytes[0..4].copy_from_slice(&width.to_le_bytes());
        param_bytes[4..8].copy_from_slice(&height.to_le_bytes());
        param_bytes[8..12].copy_from_slice(&out_width.to_le_bytes());
        param_bytes[12..16].copy_from_slice(&out_height.to_le_bytes());
        param_bytes[16..20].copy_from_slice(&factor.to_le_bytes());
        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xbrstudio xbrz params"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&params_buffer, 0, &param_bytes);

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xbrstudio xbrz output"),
            size: out_width as u64 * out_height as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("xbrstudio xbrz"),
            layout: &self.xbrz_layout,
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.lut_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("xbrstudio xbrz"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("xbrstudio xbrz"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.xbrz_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // One invocation per source pixel, every one of them writes a
            // block of `factor` times `factor` pixels.
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
            drop(pass);
        }
        self.queue.submit(Some(encoder.finish()));

        self.read_pixels(&output_buffer, out_width, out_height)
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

    #[test]
    fn the_gpu_xbrz_is_the_same_as_the_cpu() {
        let Ok(context) = GpuContext::new() else {
            return;
        };

        for image in test_images() {
            for factor in 2..=6u32 {
                let gpu = context.xbrz(&image, factor).unwrap();
                let cpu = crate::scalers::Algorithm::Xbrz
                    .scale(&image, factor)
                    .unwrap();
                assert_eq!(gpu.dimensions(), cpu.dimensions());

                // The shader is a transliteration of the algorithm, so the
                // two have to agree pixel for pixel.
                for (index, (a, b)) in gpu.pixels().zip(cpu.pixels()).enumerate() {
                    assert_eq!(
                        a.0, b.0,
                        "the GPU and the CPU differ at pixel {index} of factor {factor}"
                    );
                }
            }
        }
    }

    /// A few images that between them exercise every blend case.
    fn test_images() -> Vec<RgbaImage> {
        let mut images = Vec::new();

        // Blocks of colour with transparent texels on a diagonal, so the
        // edges, the transparency and the corners all show up.
        let mut blocks = RgbaImage::new(24, 24);
        for (x, y, pixel) in blocks.enumerate_pixels_mut() {
            let alpha = if (x + y) % 7 == 0 { 0 } else { 255 };
            let colour = match (x / 4 + y / 4) % 3 {
                0 => [20, 180, 40, alpha],
                1 => [200, 30, 60, alpha],
                _ => [40, 50, 220, alpha],
            };
            *pixel = Rgba(colour);
        }
        images.push(blocks);

        // A smooth gradient, where nothing is blended at all.
        let mut gradient = RgbaImage::new(16, 16);
        for (x, y, pixel) in gradient.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 16) as u8, (y * 16) as u8, ((x + y) * 8) as u8, 255]);
        }
        images.push(gradient);

        // Noise, which is the hardest case for the algorithm.
        let mut noise = RgbaImage::new(20, 20);
        let mut state = 0x1234_5678u32;
        for (_, _, pixel) in noise.enumerate_pixels_mut() {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *pixel = Rgba([
                (state >> 16) as u8,
                (state >> 8) as u8,
                state as u8,
                if state & 1 == 0 { 255 } else { 128 },
            ]);
        }
        images.push(noise);

        // One pixel, so every neighbour is outside the image.
        images.push(RgbaImage::from_pixel(1, 1, Rgba([10, 200, 90, 255])));

        images
    }
}
