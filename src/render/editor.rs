//! Selected artwork has its own screen-space camera, depth and material namespace.
use super::*;
impl Renderer {
    pub(super) fn draw_editor(&self, target: &wgpu::TextureView) -> Result<()> {
        if self.editor_preview.is_empty() || self.menu.is_some() || self.inventory_overlay.is_some() { return Ok(()); }
        if self.editor_preview.len() > 300_000 || !self.editor_preview.len().is_multiple_of(3)
            || self.editor_preview.iter().any(|v| v.position.iter().any(|n| !n.is_finite())
                || !(0.0..1.0).contains(&v.position[2])
                || (v.texture & 0x7fff_ffff) as usize > self.item_materials.len()) {
            return Err("Editor geometry/material boundary refused".into());
        }
        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("selected editor artwork"), contents: bytemuck::cast_slice(&self.editor_preview), usage: wgpu::BufferUsages::VERTEX,
        });
        // A separate attachment, never a world target/collider or its depth.
        let depth = Self::depth(&self.device, self.size);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("top-right live editor preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                ..Default::default()
            });
            let [uw, uh] = self.ui_viewport();
            let sx = self.size[0] as f32 / uw as f32;
            let sy = self.size[1] as f32 / uh as f32;
            pass.set_scissor_rect(((uw as f32 - 308.).max(0.) * sx) as u32, (88. * sy) as u32,
                (296. * sx) as u32, (120. * sy) as u32);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (range, material) in Self::batches(&self.editor_preview) {
                if material == 0 { pass.set_pipeline(&self.inspector_color_pipeline); }
                else {
                    let group = &self.item_materials[(material & 0x7fff_ffff) as usize - 1];
                    pass.set_pipeline(if material & 0x8000_0000 != 0 { &self.inspector_media_pipeline } else { &self.inspector_texture_pipeline });
                    pass.set_bind_group(0, &self.camera_group, &[]);
                    pass.set_bind_group(1, group, &[]);
                }
                pass.draw(range, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
}
