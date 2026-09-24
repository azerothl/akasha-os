//! Offscreen wgpu lit MeshBox viewport (edit view only).

use super::mesh::{collect_mesh_instances, ViewportCamera, UNIT_CUBE_HALF};
use crate::png::encode_rgba8_png;
use crate::scene::SceneGraph;
use bytemuck::{Pod, Zeroable};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use wgpu::util::DeviceExt;

const MAX_INSTANCES: u32 = 512;
const SAMPLE_COUNT: u32 = 1;

#[derive(Debug, Error)]
pub enum ViewportError {
    #[error("no wgpu adapter")]
    NoAdapter,
    #[error("wgpu device: {0}")]
    Device(String),
    #[error("surface/buffer map: {0}")]
    Map(String),
    #[error("encode: {0}")]
    Encode(String),
    #[error("invalid size {0}x{1}")]
    BadSize(u32, u32),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    /// Column-major world matrix (ADR 0011).
    world: [[f32; 4]; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    /// xyz = light direction; w unused (std140-safe).
    light_dir: [f32; 4],
    /// x = ambient; yzw unused.
    ambient: [f32; 4],
}

struct CachedGpuMesh {
    source: Arc<crate::mesh_asset::CpuTriangleMesh>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    texture_group: Option<wgpu::BindGroup>,
}

/// Persistent wgpu device + pipeline for the DeclUI `scene3d` edit viewport.
pub struct ViewportRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    wire_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    white_texture_group: wgpu::BindGroup,
    texture_sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    edge_index_buf: wgpu::Buffer,
    edge_index_count: u32,
    instance_buf: wgpu::Buffer,
    mesh_cache: Mutex<Vec<Arc<CachedGpuMesh>>>,
}

impl ViewportRenderer {
    pub fn new() -> Result<Self, ViewportError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, ViewportError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN
                | wgpu::Backends::GL
                | wgpu::Backends::METAL
                | wgpu::Backends::DX12,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await;
        let adapter = match adapter {
            Ok(adapter) => adapter,
            Err(_) => instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: true,
                })
                .await
                .map_err(|_| ViewportError::NoAdapter)?,
        };

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("aos-scene-viewport"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: Default::default(),
                trace: Default::default(),
                experimental_features: Default::default(),
            })
            .await
            .map_err(|e| ViewportError::Device(e.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport-lit"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport-bg"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport-texture-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("viewport-texture-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white_texture_group = make_texture_group(
            &device,
            &queue,
            &texture_layout,
            &texture_sampler,
            1,
            1,
            &[255, 255, 255, 255],
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport-pl"),
            bind_group_layouts: &[&bind_layout, &texture_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 7 => Float32x2],
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 5,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 64,
                    shader_location: 6,
                },
            ],
        };

        let targets = [Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: Some(wgpu::BlendState::REPLACE),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport-solid"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout.clone(), instance_layout.clone()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                ..Default::default()
            },
            multiview: None,
            cache: None,
        });

        let wire_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport-wire"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout, instance_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_wire"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                ..Default::default()
            },
            multiview: None,
            cache: None,
        });

        let (verts, indices, edge_indices) = unit_cube_geometry();
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube-verts"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube-indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let edge_index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube-edges"),
            contents: bytemuck::cast_slice(&edge_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (MAX_INSTANCES as u64) * (std::mem::size_of::<InstanceRaw>() as u64),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            wire_pipeline,
            bind_group,
            texture_layout,
            white_texture_group,
            texture_sampler,
            uniform_buf,
            vertex_buf,
            index_buf,
            index_count: indices.len() as u32,
            edge_index_buf,
            edge_index_count: edge_indices.len() as u32,
            instance_buf,
            mesh_cache: Mutex::new(Vec::new()),
        })
    }

    /// Render SceneGraph MeshBoxes to RGBA8 (linear-ish lit solid + wire overlay).
    pub fn render_rgba(
        &self,
        scene: &SceneGraph,
        camera: &ViewportCamera,
        width: u32,
        height: u32,
        selected: Option<&str>,
    ) -> Result<Vec<u8>, ViewportError> {
        if width < 8 || height < 8 || width > 4096 || height > 4096 {
            return Err(ViewportError::BadSize(width, height));
        }
        let aspect = width as f32 / height as f32;
        let view_proj = camera.view_proj(aspect);
        let vp = mat4_cols(&view_proj);
        let uniforms = Uniforms {
            view_proj: vp,
            light_dir: [0.35, 0.85, 0.35, 0.0],
            ambient: [0.28, 0.0, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        let instances = collect_mesh_instances(scene, selected);
        let mut cube_raw: Vec<InstanceRaw> = Vec::new();
        let mut asset_draws: Vec<(&crate::viewport::mesh::MeshInstance, InstanceRaw)> = Vec::new();
        for inst in &instances {
            if instances.len() as u32 > MAX_INSTANCES
                && cube_raw.len() + asset_draws.len() >= MAX_INSTANCES as usize
            {
                break;
            }
            let raw = InstanceRaw {
                world: mat4_cols(&inst.world),
                color: inst.color,
            };
            if inst.triangle_mesh.is_some() {
                asset_draws.push((inst, raw));
            } else {
                cube_raw.push(raw);
            }
        }
        if cube_raw.is_empty() && asset_draws.is_empty() {
            // Empty scene: still clear to atmosphere.
            cube_raw.push(InstanceRaw {
                world: [
                    [0.0, 0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 0.0],
                    [0.0, -100.0, 0.0, 1.0],
                ],
                color: [0.0, 0.0, 0.0, 0.0],
            });
        }
        let cube_count = cube_raw.len() as u32;
        if !cube_raw.is_empty() {
            self.queue
                .write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&cube_raw));
        }

        // Reuse geometry and textures while the camera moves around an asset.
        let mut asset_gpu: Vec<(Arc<CachedGpuMesh>, InstanceRaw)> = Vec::new();
        for (inst, raw) in &asset_draws {
            let Some(mesh) = &inst.triangle_mesh else {
                continue;
            };
            if let Ok(mut cache) = self.mesh_cache.lock() {
                if let Some(index) = cache.iter().position(|entry| Arc::ptr_eq(&entry.source, mesh)) {
                    let entry = cache.remove(index);
                    asset_gpu.push((Arc::clone(&entry), *raw));
                    cache.push(entry);
                    continue;
                }
            }
            let mut verts: Vec<Vertex> = Vec::with_capacity(mesh.vertex_count());
            for (i, chunk) in mesh.interleaved.as_chunks::<6>().0.iter().enumerate() {
                verts.push(Vertex {
                    position: [chunk[0], chunk[1], chunk[2]],
                    normal: [chunk[3], chunk[4], chunk[5]],
                    uv: mesh.tex_coords.get(i).copied().unwrap_or([0.0, 0.0]),
                });
            }
            let indices: Vec<u32> = mesh.indices.clone();
            if verts.is_empty() || indices.len() < 3 {
                continue;
            }
            let vbuf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("mesh-asset-verts"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let ibuf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("mesh-asset-indices"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            let texture_group = mesh.base_color_texture.as_ref().map(|texture| {
                make_texture_group(
                    &self.device,
                    &self.queue,
                    &self.texture_layout,
                    &self.texture_sampler,
                    texture.width,
                    texture.height,
                    &texture.rgba,
                )
            });
            let entry = Arc::new(CachedGpuMesh {
                source: Arc::clone(mesh),
                vertices: vbuf,
                indices: ibuf,
                index_count: indices.len() as u32,
                texture_group,
            });
            if let Ok(mut cache) = self.mesh_cache.lock() {
                if cache.len() >= 8 {
                    cache.remove(0);
                }
                cache.push(Arc::clone(&entry));
            }
            asset_gpu.push((entry, *raw));
        }
        // Pack asset instances into a small instance buffer slice after cubes.
        let asset_instance_offset =
            (cube_count as u64) * (std::mem::size_of::<InstanceRaw>() as u64);
        if !asset_gpu.is_empty() {
            let asset_raw: Vec<InstanceRaw> = asset_gpu.iter().map(|(_, raw)| *raw).collect();
            if (cube_count as usize + asset_raw.len()) <= MAX_INSTANCES as usize {
                self.queue.write_buffer(
                    &self.instance_buf,
                    asset_instance_offset,
                    bytemuck::cast_slice(&asset_raw),
                );
            }
        }

        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport-color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport-depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let bytes_per_row = aligned_bytes_per_row(width);
        let buffer_size = (bytes_per_row as u64) * (height as u64);
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport-readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("viewport-enc"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.094,
                            g: 0.110,
                            b: 0.133,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let box_count = instances
                .iter()
                .filter(|i| i.triangle_mesh.is_none())
                .count() as u32;
            if box_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_bind_group(1, &self.white_texture_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                pass.set_vertex_buffer(1, self.instance_buf.slice(..));
                pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..self.index_count, 0, 0..box_count);

                pass.set_pipeline(&self.wire_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                pass.set_vertex_buffer(1, self.instance_buf.slice(..));
                pass.set_index_buffer(self.edge_index_buf.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..self.edge_index_count, 0, 0..box_count);
            }

            for (i, (mesh, _)) in asset_gpu.iter().enumerate() {
                let inst_start = asset_instance_offset
                    + (i as u64) * (std::mem::size_of::<InstanceRaw>() as u64);
                let inst_end = inst_start + std::mem::size_of::<InstanceRaw>() as u64;
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_bind_group(
                    1,
                    mesh.texture_group.as_ref().unwrap_or(&self.white_texture_group),
                    &[],
                );
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(1, self.instance_buf.slice(inst_start..inst_end));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = output.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| ViewportError::Map(e.to_string()))?;
        rx.recv()
            .map_err(|e| ViewportError::Map(e.to_string()))?
            .map_err(|e| ViewportError::Map(e.to_string()))?;

        let data = slice.get_mapped_range();
        let row_stride = bytes_per_row as usize;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let start = y * row_stride;
            let end = start + (width as usize) * 4;
            rgba.extend_from_slice(&data[start..end]);
        }
        drop(data);
        output.unmap();
        Ok(rgba)
    }

    pub fn render_png(
        &self,
        scene: &SceneGraph,
        camera: &ViewportCamera,
        width: u32,
        height: u32,
        selected: Option<&str>,
    ) -> Result<Vec<u8>, ViewportError> {
        let rgba = self.render_rgba(scene, camera, width, height, selected)?;
        encode_rgba8_png(width, height, &rgba).map_err(ViewportError::Encode)
    }
}

fn aligned_bytes_per_row(width: u32) -> u32 {
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded.div_ceil(align) * align
}

fn make_texture_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::BindGroup {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport-base-color"),
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
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("viewport-base-color-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn mat4_cols(m: &crate::math::Mat4) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for (col, slot) in out.iter_mut().enumerate() {
        let base = col * 4;
        *slot = [m.m[base], m.m[base + 1], m.m[base + 2], m.m[base + 3]];
    }
    out
}

fn unit_cube_geometry() -> (Vec<Vertex>, Vec<u16>, Vec<u16>) {
    let h = UNIT_CUBE_HALF;
    // 24 verts: 4 per face with flat normals.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [0.0, 0.0, 1.0],
            [[-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h]],
        ),
        (
            [0.0, 0.0, -1.0],
            [[h, -h, -h], [-h, -h, -h], [-h, h, -h], [h, h, -h]],
        ),
        (
            [0.0, 1.0, 0.0],
            [[-h, h, -h], [-h, h, h], [h, h, h], [h, h, -h]],
        ),
        (
            [0.0, -1.0, 0.0],
            [[-h, -h, h], [-h, -h, -h], [h, -h, -h], [h, -h, h]],
        ),
        (
            [1.0, 0.0, 0.0],
            [[h, -h, h], [h, -h, -h], [h, h, -h], [h, h, h]],
        ),
        (
            [-1.0, 0.0, 0.0],
            [[-h, -h, -h], [-h, -h, h], [-h, h, h], [-h, h, -h]],
        ),
    ];
    let mut verts = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = verts.len() as u16;
        for p in corners {
            verts.push(Vertex {
                position: p,
                normal,
                uv: [0.0, 0.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // Wire edges use the first 8 unique corner positions (from face 0/1 verts).
    // Simpler: dedicated 8-corner buffer sharing — rebuild 8 positions + 12 edges.
    let corners = [
        [-h, -h, -h],
        [h, -h, -h],
        [h, h, -h],
        [-h, h, -h],
        [-h, -h, h],
        [h, -h, h],
        [h, h, h],
        [-h, h, h],
    ];
    // For wire we reuse solid verts poorly; instead append a second vertex region.
    let wire_base = verts.len() as u16;
    for p in corners {
        verts.push(Vertex {
            position: p,
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        });
    }
    let e = [
        (0u16, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut edge_indices = Vec::with_capacity(24);
    for (a, b) in e {
        edge_indices.push(wire_base + a);
        edge_indices.push(wire_base + b);
    }
    (verts, indices, edge_indices)
}

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var base_color_texture: texture_2d<f32>;
@group(1) @binding(1) var base_color_sampler: sampler;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec4<f32>,
    @location(7) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let world = model * vec4<f32>(v.position, 1.0);
    var o: VsOut;
    o.clip = u.view_proj * world;
    o.color = v.color;
    let nmat = mat3x3<f32>(
        model[0].xyz,
        model[1].xyz,
        model[2].xyz,
    );
    o.normal = normalize(nmat * v.normal);
    o.uv = v.uv;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light_dir.xyz);
    let ndl = max(dot(n, l), 0.0);
    let ambient = u.ambient.x;
    let lit = ambient + (1.0 - ambient) * ndl;
    let texel = textureSample(base_color_texture, base_color_sampler, i.uv);
    return vec4<f32>(i.color.rgb * texel.rgb * lit, i.color.a * texel.a);
}

@fragment
fn fs_wire(i: VsOut) -> @location(0) vec4<f32> {
    let edge = mix(i.color.rgb * 0.35, vec3<f32>(0.75, 0.82, 0.90), 0.55);
    return vec4<f32>(edge, 1.0);
}
"#;
