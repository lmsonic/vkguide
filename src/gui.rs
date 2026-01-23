use std::sync::Arc;

use ash::vk;
use egui::{DragValue, Ui, vec2};
use eyre::Ok;
use glam::{Affine3A, Quat, Vec3, Vec4};
use winit::window::Window;

use crate::{
    engine::Engine, frames::FRAMES_IN_FLIGHT, swapchain::Swapchain, utils::color_attachment_info,
    vulkan::Vulkan,
};
pub struct Gui {
    ctx: egui::Context,
    winit: egui_winit::State,
    renderer: egui_ash_renderer::Renderer,
    textures_to_free: Option<Vec<egui::TextureId>>,
}

pub fn affine_ui(ui: &mut Ui, affine: &mut Affine3A, label: &str) {
    const EULER_ROT: glam::EulerRot = glam::EulerRot::XYZ;
    let (mut scale, rotation, mut translation) = affine.to_scale_rotation_translation();
    let (euler_x, euler_y, euler_z) = rotation.to_euler(EULER_ROT);
    let mut euler_deg = Vec3::new(
        euler_x.to_degrees(),
        euler_y.to_degrees(),
        euler_z.to_degrees(),
    );
    ui.label(label);
    vec3_drag_value(ui, &mut translation, "Translation");
    vec3_drag_value(ui, &mut euler_deg, "Rotation (deg)");
    vec3_drag_value(ui, &mut scale, "Scale");
    let rotation = Quat::from_euler(
        EULER_ROT,
        euler_deg.x.to_radians(),
        euler_deg.y.to_radians(),
        euler_deg.z.to_radians(),
    );
    *affine = Affine3A::from_scale_rotation_translation(scale, rotation, translation);
}

pub fn vec4_drag_value(ui: &mut Ui, v: &mut Vec4, label: &str) {
    const SIZE: egui::Vec2 = vec2(48.0, 20.0);
    ui.label(label);
    ui.columns(4, |ui| {
        ui[0].add_sized(SIZE, DragValue::new(&mut v.x).speed(0.01));
        ui[1].add_sized(SIZE, DragValue::new(&mut v.y).speed(0.01));
        ui[2].add_sized(SIZE, DragValue::new(&mut v.z).speed(0.01));
        ui[3].add_sized(SIZE, DragValue::new(&mut v.w).speed(0.01));
    });
}
pub fn vec3_drag_value(ui: &mut Ui, v: &mut Vec3, label: &str) {
    const SIZE: egui::Vec2 = vec2(48.0, 20.0);
    ui.label(label);
    ui.columns(3, |ui| {
        ui[0].add_sized(SIZE, DragValue::new(&mut v.x).speed(0.01));
        ui[1].add_sized(SIZE, DragValue::new(&mut v.y).speed(0.01));
        ui[2].add_sized(SIZE, DragValue::new(&mut v.z).speed(0.01));
    });
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
                engine.immediate_graphics().pool(),
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
        let rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: swapchain_extent,
            })
            .color_attachments(&color_attachments)
            .layer_count(1);
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
