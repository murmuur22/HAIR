//! Real egui compositor over the visitor's shared renderer, not another material path.
use super::Workshop;
use crate::{Result, render::Renderer};

pub struct WorkshopGpu {
    pub shared: Renderer,
    painter: egui_wgpu::Renderer,
    preview: wgpu::TextureView,
    preview_id: egui::TextureId,
    revision: Option<u64>,
}
impl WorkshopGpu {
    pub async fn new(
        adapter: &wgpu::Adapter,
        format: wgpu::TextureFormat,
        _size: [u32; 2],
    ) -> Result<Self> {
        let shared = Renderer::new(
            adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [800, 600],
            &[],
        )
        .await?;
        let mut painter = egui_wgpu::Renderer::new(&shared.device, format, Default::default());
        let texture = shared.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shared HAIR scene inside egui"),
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let preview = texture.create_view(&Default::default());
        // Preserve the visitor's sRGB encoded pixels when egui samples its gamma-space UI.
        let sample = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let preview_id =
            painter.register_native_texture(&shared.device, &sample, wgpu::FilterMode::Linear);
        Ok(Self {
            shared,
            painter,
            preview,
            preview_id,
            revision: None,
        })
    }
    pub fn target(&self, size: [u32; 2]) -> wgpu::Texture {
        self.shared.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("finite actual workshop UI target"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }
    pub fn frame(
        &mut self,
        ctx: &egui::Context,
        app: &mut Workshop,
        input: egui::RawInput,
        target: &wgpu::TextureView,
        size: [u32; 2],
    ) -> Result<egui::PlatformOutput> {
        let mut output = ctx.run_ui(input, |ui| {
            app.ui(ui, app.preview.as_ref().map(|_| self.preview_id))
        });
        if let Some(theme) = &app.preview {
            if self.revision != Some(app.revision) {
                self.shared.set_theme_appearance(theme)?;
                self.shared.set_scene(&theme.vertices);
                self.revision = Some(app.revision);
            }
            if app.camera.free_fly {
                self.shared
                    .draw_inspection(&self.preview, &app.camera.pose, app.camera.height)?;
            } else {
                self.shared.draw_preview(&self.preview, &app.inspection)?;
            }
        }
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                self.painter
                    .update_texture(&self.shared.device, &self.shared.queue, id, &delta);
            }
        }
        let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        let descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: output.pixels_per_point,
        };
        let mut encoder = self
            .shared
            .device
            .create_command_encoder(&Default::default());
        let commands = self.painter.update_buffers(
            &self.shared.device,
            &self.shared.queue,
            &mut encoder,
            &jobs,
            &descriptor,
        );
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("actual workshop egui layout"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.painter
                .render(&mut pass.forget_lifetime(), &jobs, &descriptor);
        }
        self.shared
            .queue
            .submit(commands.into_iter().chain([encoder.finish()]));
        for id in output.textures_delta.free.drain() {
            self.painter.free_texture(&id);
        }
        Ok(output.platform_output)
    }
    pub fn read_pixels(&self, target: &wgpu::Texture, size: [u32; 2]) -> Result<Vec<u8>> {
        let [width, height] = size;
        let pitch = (width * 4).div_ceil(256) * 256;
        let buffer = self.shared.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("actual egui readback"),
            size: (pitch * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .shared
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target,
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
        self.shared.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.shared.wait()?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        let pixels = mapped
            .chunks(pitch as usize)
            .flat_map(|row| row[..width as usize * 4].iter().copied())
            .collect();
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }
}
