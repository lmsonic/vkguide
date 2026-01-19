use std::sync::Arc;

use ash::vk;
use eyre::Ok;
use winit::window::Window;

use crate::{
    engine::Engine,
    frames::FRAMES_IN_FLIGHT,
    swapchain::Swapchain,
    utils::{color_attachment_info, rendering_info},
    vulkan::Vulkan,
};
pub struct Gui {
    ctx: egui::Context,
    winit: egui_winit::State,
    renderer: egui_ash_renderer::Renderer,
    textures_to_free: Option<Vec<egui::TextureId>>,
}

impl Gui {
    pub fn new(window: &Window, vulkan: &Vulkan, swapchain: &Swapchain) -> eyre::Result<Self> {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let egui_winit = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            None,
            None,
            None,
        );

        let device = vulkan.device();
        let renderer = {
            let allocator = {
                let allocator_info = vk_mem::AllocatorCreateInfo::new(
                    vulkan.instance(),
                    device,
                    vulkan.physical_device(),
                );
                unsafe { vk_mem::Allocator::new(allocator_info) }?
            };
            egui_ash_renderer::Renderer::with_vk_mem_allocator(
                Arc::new(allocator),
                device.clone(),
                egui_ash_renderer::DynamicRendering {
                    color_attachment_format: swapchain.format(),
                    depth_attachment_format: None,
                },
                egui_ash_renderer::Options {
                    in_flight_frames: FRAMES_IN_FLIGHT,
                    ..Default::default()
                },
            )
        }?;
        Ok(Self {
            ctx,
            winit: egui_winit,
            renderer,
            textures_to_free: None,
        })
    }
    pub fn free_textures(&mut self) -> eyre::Result<()> {
        if let Some(textures) = self.textures_to_free.take() {
            self.renderer.free_textures(&textures)?;
        }
        Ok(())
    }

    pub fn generate_ui(
        &mut self,
        engine: &mut Engine,
    ) -> eyre::Result<(Vec<egui::ClippedPrimitive>, f32)> {
        let raw_input = self.winit.take_egui_input(engine.window());
        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = self.ctx.run(raw_input, |ctx| engine.build_ui(ctx));
        self.winit
            .handle_platform_output(engine.window(), platform_output);
        if !textures_delta.free.is_empty() {
            self.textures_to_free = Some(textures_delta.free);
        }
        if !textures_delta.set.is_empty() {
            self.renderer.set_textures(
                engine.vulkan().graphics_queue(),
                engine.immediate_submit().pool(),
                &textures_delta.set,
            )?;
        }
        Ok((
            self.ctx.tessellate(shapes, pixels_per_point),
            pixels_per_point,
        ))
    }

    pub fn draw_gui(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        target_image_view: vk::ImageView,
        swapchain_extent: vk::Extent2D,
        pixels_per_point: f32,
        primitives: &[egui::ClippedPrimitive],
    ) -> eyre::Result<()> {
        let color_attachment = color_attachment_info().view(target_image_view).call();
        let color_attachments = [color_attachment];
        let rendering_info = rendering_info()
            .render_extent(swapchain_extent)
            .color_attachments(&color_attachments)
            .call();
        unsafe { device.cmd_begin_rendering(cmd, &rendering_info) };

        self.renderer
            .cmd_draw(cmd, swapchain_extent, pixels_per_point, primitives)?;
        unsafe { device.cmd_end_rendering(cmd) };
        Ok(())
    }

    pub const fn renderer_mut(&mut self) -> &mut egui_ash_renderer::Renderer {
        &mut self.renderer
    }

    pub const fn winit_mut(&mut self) -> &mut egui_winit::State {
        &mut self.winit
    }
}
