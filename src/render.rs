pub mod web_feedback;
use crate::{Pose, Result, World, assets::Vertex};
use std::time::Duration;
use wgpu::util::DeviceExt;

// The actual final capture consumer, also exercised without GPU timing in regression tests.
fn write_capture(
    output: &crate::resources::SmokeOutput,
    name: &str,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<()> {
    use std::io::Write;
    let file = output.create_capture(name)?;
    let mut writer = std::io::BufWriter::new(file);
    let mut png = png::Encoder::new(&mut writer, width, height);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    let mut png = png
        .write_header()
        .map_err(|e| format!("smoke output {name}: {e}"))?;
    png.write_image_data(pixels)
        .map_err(|e| format!("smoke output {name}: {e}"))?;
    png.finish()
        .map_err(|e| format!("smoke output {name}: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("smoke output {name}: {e}"))?;
    Ok(())
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    world_pipeline: wgpu::RenderPipeline,
    object_pipeline: wgpu::RenderPipeline,
    material_sources: Vec<std::sync::Arc<crate::assets::Texture>>,
    sky: crate::sky::SkyAppearance,
    sky_pipeline: wgpu::RenderPipeline,
    sky_material: Option<wgpu::BindGroup>,
    sky_rays: wgpu::Buffer,
    sky_rays_group: wgpu::BindGroup,
    target_is_srgb: bool,
    ui_pipeline: wgpu::RenderPipeline,
    media_pipeline: wgpu::RenderPipeline,
    viewer: Option<(std::sync::Arc<crate::assets::Texture>, wgpu::BindGroup)>,
    pub viewer_visible: bool,
    pub viewer_view: [f32; 3],
    camera: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    geometry: wgpu::Buffer,
    batches: Vec<(std::ops::Range<u32>, u32)>,
    materials: Vec<wgpu::BindGroup>,
    depth: wgpu::TextureView,
    pub size: [u32; 2],
    /// Session-only requested screen-space UI scale percentage (HUD, not 3D).
    pub ui_scale: u16,
    /// Open presentation menu, drawn last over every other screen-space layer.
    pub menu: Option<crate::presentation::Menu>,
    pub exploration: Option<(
        std::sync::Arc<crate::exploration::Map>,
        crate::exploration::MapMode,
    )>,
    pub pack_feedback: String,
    /// Literal input, displayed error and rows scrolled up from the cursor tail.
    pub address_entry: Option<(String, Option<String>, usize)>,
    web_feedback_active: bool,
    web_feedback_vertices: Vec<Vertex>,
    web_feedback_buffer: wgpu::Buffer,
}
pub fn instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle())
}
pub async fn adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: surface,
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            ..Default::default()
        })
        .await
        .map_err(|e| format!("adapter: {e}"))?;
    println!("ADAPTER {:?}", adapter.get_info());
    Ok(adapter)
}
impl Renderer {
    pub async fn new(
        adapter: &wgpu::Adapter,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        vertices: &[Vertex],
    ) -> Result<Self> {
        if size.contains(&0) || size.iter().any(|n| *n > 4096) {
            return Err("render dimensions must be 1..4096".into());
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("HAIR device"),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("device: {e}"))?;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("office + custom text"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2],
        };
        let make_pipeline =
            |entry: &str, fragment: &str, depth: bool, explicit: Option<&wgpu::PipelineLayout>| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: explicit,
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        buffers: &[Some(layout.clone())],
                    },
                    primitive: wgpu::PrimitiveState {
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: depth.then_some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(wgpu::CompareFunction::Less),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fragment),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shared camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shared material layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        // Both sampled pipelines use explicit layouts. Auto-derived layouts are
        // pipeline-exclusive and must never be transplanted to another pipeline.
        let shared_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world and screen-space shared material layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
            ..Default::default()
        });
        let world_pipeline = make_pipeline("world_vs", "world_fs", true, Some(&shared_layout));
        let ui_pipeline = make_pipeline("ui_vs", "ui_fs", false, None);
        let media_pipeline = make_pipeline(
            "ui_vs",
            if format.is_srgb() {
                "media_fs"
            } else {
                "media_unorm_fs"
            },
            false,
            Some(&shared_layout),
        );
        let object_pipeline = make_pipeline(
            "world_vs",
            if format.is_srgb() {
                "media_fs"
            } else {
                "media_unorm_fs"
            },
            true,
            Some(&shared_layout),
        );
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bounded static panorama"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky at infinity, before world and UI"),
            layout: Some(&shared_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("sky_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some(if format.is_srgb() {
                    "sky_fs"
                } else {
                    "sky_unorm_fs"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sky_rays = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("translation-free panorama ray basis"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky_rays_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("panorama rays"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sky_rays.as_entire_binding(),
            }],
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera pose"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &world_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        if let Some(error) = scope.pop().await {
            return Err(format!("renderer validation: {error}"));
        }
        let geometry = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("actual Kenney scene"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let depth = Self::depth(&device, size);
        let web_feedback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded web feedback overlay"),
            size: (web_feedback::MAX_VERTICES * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        println!("SCENE_VERTICES={}", vertices.len());
        let mut renderer = Self {
            device,
            queue,
            world_pipeline,
            object_pipeline,
            material_sources: Vec::new(),
            sky: crate::sky::SkyAppearance::Legacy,
            sky_pipeline,
            sky_material: None,
            sky_rays,
            sky_rays_group,
            target_is_srgb: format.is_srgb(),
            ui_pipeline,
            media_pipeline,
            viewer: None,
            viewer_visible: false,
            viewer_view: [1.0, 0.0, 0.0],
            camera,
            camera_group,
            geometry,
            batches: Self::batches(vertices),
            materials: Vec::new(),
            depth,
            size,
            ui_scale: 100,
            menu: None,
            exploration: None,
            pack_feedback: String::new(),
            address_entry: None,
            web_feedback_active: false,
            web_feedback_vertices: Vec::with_capacity(web_feedback::MAX_VERTICES),
            web_feedback_buffer,
        };
        renderer.set_materials(&[])?;
        Ok(renderer)
    }
    fn batches(vertices: &[Vertex]) -> Vec<(std::ops::Range<u32>, u32)> {
        let mut out: Vec<(std::ops::Range<u32>, u32)> = Vec::new();
        for (i, v) in vertices.iter().enumerate() {
            if let Some((range, texture)) = out.last_mut()
                && *texture == v.texture
            {
                range.end = i as u32 + 1;
            } else {
                out.push((i as u32..i as u32 + 1, v.texture));
            }
        }
        out
    }
    /// Stage both resource classes before committing selected appearance.
    /// Sky uses no scene/viewer slot and retains at most one active GPU image.
    /// Failure retains materials, sky and viewer. No source acquisition on T.
    pub fn set_theme_appearance(&mut self, theme: &crate::theme::Theme) -> Result<()> {
        use crate::sky::SkyAppearance;
        theme.sky.validate()?;
        let sky_material = if let SkyAppearance::Panorama(image) = &theme.sky {
            if let SkyAppearance::Panorama(old) = &self.sky
                && std::sync::Arc::ptr_eq(old, image)
                && self.sky_material.is_some()
            {
                self.sky_material.clone()
            } else {
                let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("panorama longitude repeat, latitude clamp"),
                    address_mode_u: wgpu::AddressMode::Repeat,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                Some(self.upload_material(image.image(), &sampler)?)
            }
        } else {
            None
        };
        self.set_materials(&theme.textures)?;
        self.sky = theme.sky.clone();
        self.sky_material = sky_material;
        Ok(())
    }
    /// Material-only refreshes (including media polling) must retain selected sky.
    pub fn set_materials(
        &mut self,
        textures: &[std::sync::Arc<crate::assets::Texture>],
    ) -> Result<()> {
        let bytes = textures.iter().map(|t| t.rgba.len()).sum::<usize>();
        let viewer_bytes = self.viewer.as_ref().map_or(0, |(t, _)| t.rgba.len());
        if textures.len() + usize::from(self.viewer.is_some()) > 64
            || bytes + viewer_bytes > 32_000_000
        {
            return Err("scene plus viewer texture budget exceeds 64 / 32 MB".into());
        }
        if !self.materials.is_empty()
            && textures.len() == self.material_sources.len()
            && textures
                .iter()
                .zip(&self.material_sources)
                .all(|(a, b)| std::sync::Arc::ptr_eq(a, b))
        {
            return Ok(());
        }
        let white = crate::assets::Texture {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut materials = vec![match self.materials.first() {
            Some(white) => white.clone(),
            None => self.upload_material(&white, &sampler)?,
        }];
        for image in textures {
            if let Some(index) = self
                .material_sources
                .iter()
                .position(|old| std::sync::Arc::ptr_eq(old, image))
            {
                materials.push(self.materials[index + 1].clone());
            } else {
                materials.push(self.upload_material(image, &sampler)?);
            }
        }
        self.materials = materials;
        self.material_sources = textures.to_vec();
        Ok(())
    }
    fn upload_material(
        &self,
        image: &crate::assets::Texture,
        sampler: &wgpu::Sampler,
    ) -> Result<wgpu::BindGroup> {
        if image.width == 0
            || image.height == 0
            || image.width > 1024
            || image.height > 1024
            || image.rgba.len() != (image.width * image.height * 4) as usize
        {
            return Err("invalid scene texture dimensions/bytes".into());
        }
        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("shared base color sRGB"),
                size: wgpu::Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &image.rgba,
        );
        Ok(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shared base color material"),
            layout: &self.world_pipeline.get_bind_group_layout(1),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        }))
    }
    pub fn clear_viewer(&mut self) {
        self.viewer = None;
        self.viewer_visible = false;
    }
    /// At most one additional shared material, counted WITH theme textures;
    /// white is implicit zero, scene materials remain one-based and repeat-wrapped.
    pub fn set_viewer_texture(
        &mut self,
        image: Option<&std::sync::Arc<crate::assets::Texture>>,
    ) -> Result<()> {
        let Some(image) = image else {
            self.clear_viewer();
            return Ok(());
        };
        if self
            .viewer
            .as_ref()
            .is_some_and(|(old, _)| std::sync::Arc::ptr_eq(old, image))
        {
            return Ok(());
        }
        if self.materials.len().saturating_sub(1) + 1 > 64
            || self
                .material_sources
                .iter()
                .map(|t| t.rgba.len())
                .sum::<usize>()
                + image.rgba.len()
                > 32_000_000
        {
            return Err("viewer plus theme exceeds 64 / 32 MB texture budget".into());
        }
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("UI media edge clamp; pack repeat unchanged"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let group = self.upload_material(image, &sampler)?;
        self.viewer = Some((image.clone(), group));
        Ok(())
    }
    /// Update just the fixed feedback buffer; existing room/material/sky ownership
    /// and all staging/work/source guards remain untouched.
    pub fn set_web_feedback(&mut self, snapshot: &web_feedback::Snapshot, context_owned: bool) {
        let viewport = self.ui_viewport();
        snapshot.paint(&mut self.web_feedback_vertices, viewport, context_owned);
        self.web_feedback_active = !self.web_feedback_vertices.is_empty();
        if self.web_feedback_active {
            self.queue.write_buffer(
                &self.web_feedback_buffer,
                0,
                bytemuck::cast_slice(&self.web_feedback_vertices),
            );
        }
    }
    pub fn set_scene(&mut self, vertices: &[Vertex]) {
        self.geometry = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("active content/theme scene"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.batches = Self::batches(vertices);
    }
    fn depth(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }
    /// Logical screen-space viewport: HUD layouts use it with unchanged
    /// normalized vertices, so each logical pixel spans the applied UI scale.
    pub fn ui_viewport(&self) -> [u32; 2] {
        crate::presentation::ui_viewport(self.size, self.ui_scale)
    }
    pub fn resize(&mut self, size: [u32; 2]) {
        self.size = size;
        self.depth = Self::depth(&self.device, size);
    }
    pub fn draw(
        &self,
        target: &wgpu::TextureView,
        pose: &Pose,
        world: &World,
        scroll: usize,
    ) -> Result<()> {
        self.draw_frame(target, pose, Some(world), scroll, 1.65)
    }
    /// The workshop uses the visitor's exact material/world pipeline without its HUD.
    pub fn draw_preview(&self, target: &wgpu::TextureView, pose: &Pose) -> Result<()> {
        self.draw_frame(target, pose, None, 0, 1.65)
    }
    /// Session-only Workshop camera height; visitor pose/collision remain unchanged.
    #[cfg(feature = "workshop")]
    pub fn draw_inspection(
        &self,
        target: &wgpu::TextureView,
        pose: &Pose,
        height: f32,
    ) -> Result<()> {
        self.draw_frame(target, pose, None, 0, height)
    }
    fn sky_clear(&self) -> wgpu::Color {
        match &self.sky {
            // Do not encode or round-trip Legacy: this is the exact historical clear,
            // including its different appearance on non-sRGB output targets.
            // Panorama covers this clear in its own full-screen pass.
            crate::sky::SkyAppearance::Legacy | crate::sky::SkyAppearance::Panorama(_) => {
                wgpu::Color {
                    r: 0.04,
                    g: 0.06,
                    b: 0.05,
                    a: 1.0,
                }
            }
            crate::sky::SkyAppearance::Colour(rgb) => {
                // sRGB targets encode the linear clear in hardware. SDR non-sRGB
                // targets need one explicit transfer, matching the media policy.
                let [r, g, b] = rgb.map(|v| {
                    if self.target_is_srgb {
                        v
                    } else if v <= 0.0031308 {
                        12.92 * v
                    } else {
                        1.055 * v.powf(1.0 / 2.4) - 0.055
                    }
                });
                wgpu::Color { r, g, b, a: 1.0 }
            }
        }
    }
    fn draw_frame(
        &self,
        target: &wgpu::TextureView,
        pose: &Pose,
        world: Option<&World>,
        scroll: usize,
        eye_height: f32,
    ) -> Result<()> {
        if self.batches.len() > 4096
            || self.batches.iter().any(|(r, t)| {
                (*t & 0x7fff_ffff) as usize >= self.materials.len()
                    || !r.start.is_multiple_of(3)
                    || !r.end.is_multiple_of(3)
            })
        {
            return Err(
                "invalid scene material/triangle/draw budget; no silent texture fallback".into(),
            );
        }
        use glam::{Mat4, Vec3};
        let eye = Vec3::new(pose.position[0], eye_height, pose.position[1]);
        let direction = Vec3::new(
            pose.yaw.sin() * pose.pitch.cos(),
            pose.pitch.sin(),
            -pose.yaw.cos() * pose.pitch.cos(),
        );
        let matrix = Mat4::perspective_rh(
            65.0_f32.to_radians(),
            self.size[0] as f32 / self.size[1] as f32,
            0.05,
            50.0,
        ) * Mat4::look_to_rh(eye, direction, Vec3::Y);
        self.queue.write_buffer(
            &self.camera,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        if self.sky_material.is_some() {
            // Same yaw/pitch/FOV convention as the world view, deliberately no
            // eye position or inverse translated matrix: exact translation invariance.
            let scale = (65.0_f32.to_radians() * 0.5).tan();
            let right = direction.cross(Vec3::Y).normalize();
            let up = right.cross(direction);
            let rays = [
                (right * (self.size[0] as f32 / self.size[1] as f32) * scale)
                    .extend(0.0)
                    .to_array(),
                (up * scale).extend(0.0).to_array(),
                direction.extend(0.0).to_array(),
            ];
            self.queue
                .write_buffer(&self.sky_rays, 0, bytemuck::cast_slice(&rays));
        }
        // Screen-space layers lay out in the logical viewport; 3D/camera stay physical.
        let [ui_w, ui_h] = self.ui_viewport();
        let media = self.viewer.as_ref().filter(|_| {
            self.viewer_visible
                && self.address_entry.is_none()
                && world.is_some_and(|w| w.reading && w.folder_view.is_some())
                && ui_w >= 64
                && ui_h >= 180
        });
        let mut ui = world
            .map(|world| {
                if media.is_some() {
                    crate::text::image_overlay(world, ui_w, ui_h)
                } else {
                    crate::text::overlay_with_web_feedback(
                        world,
                        ui_w,
                        ui_h,
                        scroll,
                        self.web_feedback_active,
                    )
                }
            })
            .unwrap_or_default();
        if let Some((map, mode)) = &self.exploration {
            // Yield the whole map, without changing its retained mode or state.
            // Feedback replaces the portal panel, so only a visible panel owns space.
            let disclosure_owns_space = world.is_some_and(|world| {
                (!self.web_feedback_active
                    && !crate::text::portal_overlay(world, ui_w, ui_h).is_empty())
                    || (*mode == crate::exploration::MapMode::Expanded && !world.tree_collapsed)
            });
            if !disclosure_owns_space {
                ui.extend(crate::text::exploration_map::overlay(
                    map, *mode, pose.yaw, ui_w, ui_h,
                ));
            }
        }
        // Reader and transient address entry own their controls; global status
        // must not split the reader footer or compete with entry recovery help.
        if world.is_some_and(|world| !world.reading) && self.address_entry.is_none() {
            ui.extend(crate::text::pack_overlay(&self.pack_feedback, ui_w, ui_h));
        }
        if world.is_some()
            && let Some((raw, error, back)) = &self.address_entry
        {
            ui.extend(crate::text::address_overlay_scrolled(
                raw,
                error.as_deref(),
                *back,
                ui_w,
                ui_h,
            ));
        }
        let ui_buffer = (!ui.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("bitmap text quads"),
                    contents: bytemuck::cast_slice(&ui),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let media_buffer = media.map(|(image, _)| {
            let [w, h] = [ui_w, ui_h].map(|v| v as f32);
            let scale = ((w - 32.0) / image.width as f32).min((h - 154.0) / image.height as f32);
            let [zoom, pan_x, pan_y] = self.viewer_view;
            let zoom = if zoom.is_finite() {
                // Same 1/8..8 fit-relative range as the opened-reader controls.
                zoom.clamp(0.125, 8.0)
            } else {
                1.0
            };
            let pan = |v: f32| {
                if v.is_finite() {
                    v.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            };
            let iw = image.width as f32 * scale * zoom;
            let ih = image.height as f32 * scale * zoom;
            let left = (w - iw) * 0.5 - pan(pan_x) * ((iw - (w - 32.0)) * 0.5).max(0.0);
            let top =
                68.0 + (h - 154.0 - ih) * 0.5 - pan(pan_y) * ((ih - (h - 154.0)) * 0.5).max(0.0);
            let vertex = |x: f32, y: f32, uv| Vertex {
                position: [2.0 * x / w - 1.0, 1.0 - 2.0 * y / h, 0.0],
                color: [1.0; 3],
                uv,
                texture: 0,
            };
            let a = vertex(left, top, [0.0, 0.0]);
            let b = vertex(left + iw, top, [1.0, 0.0]);
            let c = vertex(left + iw, top + ih, [1.0, 1.0]);
            let d = vertex(left, top + ih, [0.0, 1.0]);
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("proportional decoded image quad"),
                    contents: bytemuck::cast_slice(&[a, b, c, a, c, d]),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if let Some(group) = &self.sky_material {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("held panorama at infinity"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.sky_clear()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.sky_rays_group, &[]);
            pass.set_bind_group(1, group, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("office"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if self.sky_material.is_some() {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(self.sky_clear())
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.world_pipeline);
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_vertex_buffer(0, self.geometry.slice(..));
            for (range, material) in &self.batches {
                pass.set_pipeline(if material & 0x8000_0000 != 0 {
                    &self.object_pipeline
                } else {
                    &self.world_pipeline
                });
                pass.set_bind_group(1, &self.materials[(material & 0x7fff_ffff) as usize], &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        if let Some(ui_buffer) = ui_buffer {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reader HUD"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.ui_pipeline);
            pass.set_vertex_buffer(0, ui_buffer.slice(..));
            pass.draw(0..ui.len() as u32, 0..1);
            if world.is_some() && self.web_feedback_active {
                pass.set_vertex_buffer(0, self.web_feedback_buffer.slice(..));
                pass.draw(0..self.web_feedback_vertices.len() as u32, 0..1);
            }
        }
        if let (Some((_, group)), Some(buffer)) = (media, media_buffer.as_ref()) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opened PNG shared textured UI"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.media_pipeline);
            // Same logical frame as the quad above, mapped to physical pixels.
            let [sx, sy] = [0, 1].map(|i| self.size[i] as f32 / [ui_w, ui_h][i] as f32);
            let left = ((16.0 * sx) as u32).min(self.size[0] - 1);
            let top = ((68.0 * sy) as u32).min(self.size[1] - 1);
            pass.set_scissor_rect(
                left,
                top,
                (((ui_w - 32) as f32 * sx) as u32).clamp(1, self.size[0] - left),
                (((ui_h - 154) as f32 * sy) as u32).clamp(1, self.size[1] - top),
            );
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_bind_group(1, group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..6, 0..1);
        }
        // The paused menu is drawn last so no reader/media/feedback layer covers it.
        let menu = self
            .menu
            .filter(|_| world.is_some())
            .map(|menu| crate::presentation::overlay(&menu, self.ui_scale, self.size))
            .filter(|ui| !ui.is_empty());
        if let Some(menu) = &menu {
            let buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("presentation menu quads"),
                    contents: bytemuck::cast_slice(menu),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("presentation menu"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.ui_pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..menu.len() as u32, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
    pub fn wait(&self) -> Result<()> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(15)),
            })
            .map_err(|e| format!("GPU wait: {e}"))?;
        Ok(())
    }
    pub fn capture(
        &self,
        output: &crate::resources::SmokeOutput,
        name: &str,
        pose: &Pose,
        world: &World,
    ) -> Result<()> {
        let [width, height] = self.size;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("real GPU capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.draw(&texture.create_view(&Default::default()), pose, world, 0)?;
        let pitch = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture readback"),
            size: (pitch * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.wait()?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        let pixels: Vec<u8> = mapped
            .chunks(pitch as usize)
            .flat_map(|row| row[..width as usize * 4].iter().copied())
            .collect();
        let unique: std::collections::HashSet<_> = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2]])
            .collect();
        // The bitmap reader deliberately uses a small palette; scene diversity is
        // not a valid reader-visibility criterion. Still reject a uniform frame.
        let minimum_colors = if world.reading { 2 } else { 20 };
        if unique.len() < minimum_colors {
            return Err(format!(
                "GPU image suspiciously flat: {} colors",
                unique.len()
            ));
        }
        write_capture(output, name, width, height, &pixels)?;
        drop(mapped);
        buffer.unmap();
        println!(
            "GPU_IMAGE {} {}x{} colors={}",
            output.path().join(name).display(),
            width,
            height,
            unique.len()
        );
        Ok(())
    }
    pub async fn error_probe(&self) -> Result<()> {
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _bad = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("intentional negative control"),
                source: wgpu::ShaderSource::Wgsl("this is not valid WGSL".into()),
            });
        let error = scope.pop().await.ok_or("invalid shader was not rejected")?;
        println!("EXPECTED_GPU_ERROR {error}");
        Ok(())
    }
}

#[cfg(test)]
#[path = "render_output_tests.rs"]
mod output_tests;
