use crate::model::MapDocument;
use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use eframe::{egui, egui_wgpu};
use egui_wgpu::wgpu;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    map_size: [u32; 2],
    _pad0: [u32; 2],
    view_origin: [f32; 2],
    view_size: [f32; 2],
    time: f32,
    blend_strength: f32,
    grid_opacity: f32,
    _pad1: f32,
}

pub struct GpuMapRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    tile_buffer: wgpu::Buffer,
    globals_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    color_view: wgpu::TextureView,
    _color_tex: wgpu::Texture,
    texture_id: egui::TextureId,
    doc_dims: [u32; 2],
}

impl GpuMapRenderer {
    pub fn new(render_state: &egui_wgpu::RenderState, doc: &MapDocument) -> Result<Self> {
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let target_format = render_state.target_format;
        let target_px = [1600, 900];
        let doc_dims = doc.dims();

        let tile_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-buffer"),
            size: (doc.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals-buffer"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("map-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group =
            Self::make_bind_group(&device, &bind_group_layout, &globals_buffer, &tile_buffer);
        let shader = device.create_shader_module(wgpu::include_wgsl!("terrain_shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("map-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("map-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let (color_tex, color_view) = Self::create_target(&device, target_format, target_px);
        let texture_id = render_state.renderer.write().register_native_texture(
            &device,
            &color_view,
            wgpu::FilterMode::Linear,
        );

        let me = Self {
            device,
            queue,
            tile_buffer,
            globals_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            color_view,
            _color_tex: color_tex,
            texture_id,
            doc_dims,
        };
        me.upload_tiles(doc);
        Ok(me)
    }

    pub fn texture_id(&self) -> egui::TextureId {
        self.texture_id
    }

    pub fn render(
        &mut self,
        doc: &MapDocument,
        upload_tiles: bool,
        view_origin: [f32; 2],
        view_size: [f32; 2],
        time_seconds: f32,
        blend_strength: f32,
        grid_opacity: f32,
    ) {
        if doc.dims() != self.doc_dims {
            self.rebuild_tile_buffer(doc);
        }
        if upload_tiles {
            self.upload_tiles(doc);
        }

        let globals = Globals {
            map_size: [doc.width, doc.height],
            _pad0: [0; 2],
            view_origin,
            view_size,
            time: time_seconds,
            blend_strength,
            grid_opacity,
            _pad1: 0.0,
        };
        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("map-render-encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("map-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.06,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit([encoder.finish()]);
    }

    fn rebuild_tile_buffer(&mut self, doc: &MapDocument) {
        self.tile_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-buffer"),
            size: (doc.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.bind_group = Self::make_bind_group(
            &self.device,
            &self.bind_group_layout,
            &self.globals_buffer,
            &self.tile_buffer,
        );
        self.doc_dims = doc.dims();
        self.upload_tiles(doc);
    }

    fn upload_tiles(&self, doc: &MapDocument) {
        let packed = doc.packed_tiles();
        self.queue
            .write_buffer(&self.tile_buffer, 0, bytemuck::cast_slice(&packed));
    }

    fn create_target(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        target_px: [u32; 2],
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let color_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("map-render-target"),
            size: wgpu::Extent3d {
                width: target_px[0],
                height: target_px[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());
        (color_tex, color_view)
    }

    fn make_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        globals_buffer: &wgpu::Buffer,
        tile_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("map-bind-group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tile_buffer.as_entire_binding(),
                },
            ],
        })
    }
}
