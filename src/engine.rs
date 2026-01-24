use std::{mem::ManuallyDrop, sync::Arc};

use ash::vk::{self};
use eyre::eyre;
use glam::{Affine3A, Mat4, Vec3};
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{
    compute::{ComputeEffect, create_compute_effects},
    descriptors::{DescriptorAllocator, PoolSizeRatio},
    frames::Frames,
    graphics::MeshPipeline,
    gui::{Gui, affine_ui, vec4_drag_value},
    immediate::ImmediateSubmit,
    mesh::{GPUDrawPushConstants, Mesh, load_gltf_from_path},
    shader::ShaderCompiler,
    swapchain::{self, Swapchain},
    texture::{AllocatedImage, DrawImage, copy_image_to_image, create_depth_image},
    utils::{
        color_attachment_info, depth_attachment_info, semaphore_submit_info, transition_image,
    },
    vulkan::Vulkan,
};

pub struct Engine {
    window: Arc<Window>,
    pub render: bool,
    vulkan: Vulkan,
    allocator: ManuallyDrop<vk_mem::Allocator>,
    swapchain: Swapchain,
    render_semaphores: Vec<vk::Semaphore>,
    frames: Frames,
    shader_compiler: ShaderCompiler,
    descriptor_allocator: DescriptorAllocator,
    draw_image: DrawImage,
    render_scale: f32,
    depth_image: AllocatedImage,
    immediate_transfer: ImmediateSubmit,
    immediate_graphics: ImmediateSubmit,
    background_effects: Vec<ComputeEffect>,
    current_background_effect: usize,
    mesh_pipeline: MeshPipeline,
    mesh_matrix: Affine3A,
    meshes: Vec<Mesh>,
    resize_swapchain: bool,
}

impl Engine {
    pub fn destroy(&mut self, gui: &mut ManuallyDrop<Gui>) {
        unsafe { self.vulkan.device().device_wait_idle() }.unwrap();
        let device = self.vulkan.device();
        let allocator = &mut self.allocator;
        self.frames.destroy(device);
        //
        for mesh in &mut self.meshes {
            mesh.mesh_buffers_mut().destroy(allocator);
        }
        self.mesh_pipeline.destroy(device);
        unsafe { ManuallyDrop::drop(gui) };
        self.immediate_graphics.destroy(device);
        self.immediate_transfer.destroy(device);
        for e in &mut self.background_effects {
            e.destroy(device);
        }
        self.descriptor_allocator.destroy_pool(device);
        self.depth_image.destroy(device, allocator);
        self.draw_image.destroy(device, allocator);

        unsafe { ManuallyDrop::drop(allocator) };
        //

        let swapchain_device = self.vulkan.swapchain_device();
        self.swapchain.destroy(device, &swapchain_device);
        for s in &self.render_semaphores {
            unsafe { device.destroy_semaphore(*s, None) };
        }
        unsafe { device.destroy_device(None) };

        let surface_instance = self.vulkan.surface_instance();
        unsafe { surface_instance.destroy_surface(self.vulkan.surface(), None) };

        let debug_instance = self.vulkan.debug_instance();
        unsafe {
            debug_instance.destroy_debug_utils_messenger(self.vulkan.debug_messenger(), None);
        };

        let instance = self.vulkan.instance();
        unsafe { instance.destroy_instance(None) };
    }
    pub fn new(window: Arc<Window>) -> eyre::Result<Self> {
        let vulkan = Vulkan::new(&window)?;
        let PhysicalSize { width, height } = window.inner_size();

        let swapchain = Swapchain::new(
            width,
            height,
            &vulkan,
            swapchain::IMAGE_FORMAT,
            swapchain::COLOR_SPACE,
            vk::PresentModeKHR::FIFO,
            vk::ImageUsageFlags::TRANSFER_DST,
        )?;
        let device = vulkan.device();
        let render_semaphores = swapchain.create_render_semaphores(device)?;
        let frames = Frames::new(&vulkan)?;

        let mut allocator_info =
            vk_mem::AllocatorCreateInfo::new(vulkan.instance(), device, vulkan.physical_device());
        allocator_info.flags = vk_mem::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;
        let allocator = unsafe { vk_mem::Allocator::new(allocator_info) }?;
        let shader_compiler = ShaderCompiler::new()?;

        let descriptor_allocator = DescriptorAllocator::new(
            device,
            10,
            &[PoolSizeRatio::new(vk::DescriptorType::STORAGE_IMAGE, 1.0)],
        )?;
        const MONITOR_WIDTH: u32 = 1980;
        const MONITOR_HEIGHT: u32 = 1080;
        let draw_image = DrawImage::new(
            MONITOR_WIDTH,
            MONITOR_HEIGHT,
            device,
            &allocator,
            &descriptor_allocator,
        )?;
        let depth_image = create_depth_image(device, &allocator, &draw_image)?;
        let immediate_graphics =
            ImmediateSubmit::new(device, vulkan.queue_family_indices().graphics)?;
        let immediate_transfer =
            ImmediateSubmit::new(device, vulkan.queue_family_indices().transfer)?;
        let background_effects = create_compute_effects(device, &draw_image, &shader_compiler)?;

        let mesh_pipeline = MeshPipeline::new(device, &shader_compiler, &draw_image, &depth_image)?;

        let meshes = load_gltf_from_path(
            "assets/basicmesh.glb",
            device,
            &allocator,
            vulkan.transfer_queue(),
            &immediate_transfer,
        )?;
        Ok(Self {
            window,
            render: true,
            vulkan,
            swapchain,
            render_semaphores,

            frames,
            allocator: ManuallyDrop::new(allocator),
            draw_image,
            depth_image,
            shader_compiler,
            descriptor_allocator,
            background_effects,
            current_background_effect: 0,
            immediate_transfer,
            immediate_graphics,
            mesh_pipeline,
            mesh_matrix: Affine3A::from_translation(Vec3::new(0.0, 0.0, -5.0)),
            meshes,
            render_scale: 1.0,
            resize_swapchain: false,
        })
    }
    fn draw_extent(&self) -> vk::Extent2D {
        let draw_extent = self.draw_image.extent();
        let swapchain_extent = self.swapchain.extent();
        let width = draw_extent.width.min(swapchain_extent.width) as f32 * self.render_scale;
        let height = draw_extent.height.min(swapchain_extent.height) as f32 * self.render_scale;
        vk::Extent2D {
            width: width.round() as u32,
            height: height.round() as u32,
        }
    }

    fn draw_background(&self, cmd: vk::CommandBuffer) {
        let device = self.vulkan.device();
        let background_effect = &self.background_effects[self.current_background_effect];
        unsafe {
            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                background_effect.pipeline(),
            );
        };
        unsafe {
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                background_effect.layout(),
                0,
                &[self.draw_image.descriptor_set()],
                &[],
            );
        };

        let push_constant = background_effect.data;
        let push_constants_bytes = bytemuck::bytes_of(&push_constant);
        unsafe {
            device.cmd_push_constants(
                cmd,
                background_effect.layout(),
                vk::ShaderStageFlags::COMPUTE,
                0,
                push_constants_bytes,
            );
        };
        let draw_extent = self.draw_extent();
        unsafe {
            device.cmd_dispatch(
                cmd,
                draw_extent.width.div_ceil(16),
                draw_extent.height.div_ceil(16),
                1,
            );
        };
    }

    pub(crate) fn build_ui(&mut self, ctx: &egui::Context) {
        let background_effects_len = self.background_effects.len();
        let selected = &mut self.background_effects[self.current_background_effect];
        egui::Window::new("Background").show(ctx, |ui| {
            ui.label(selected.name());
            let slider = egui::Slider::new(
                &mut self.current_background_effect,
                0..=background_effects_len - 1,
            );
            ui.add(slider.text("Effect Index"));
            vec4_drag_value(ui, &mut selected.data.data1, "data1");
            vec4_drag_value(ui, &mut selected.data.data2, "data2");
            vec4_drag_value(ui, &mut selected.data.data3, "data3");
            vec4_drag_value(ui, &mut selected.data.data4, "data4");

            affine_ui(ui, &mut self.mesh_matrix, "Mesh Matrix");
            ui.add(egui::Slider::new(&mut self.render_scale, 0.3..=1.0))
        });
    }
    fn draw_geometry(&self, cmd: vk::CommandBuffer) {
        let device = self.vulkan.device();
        let color_attachment_info = color_attachment_info()
            .view(self.draw_image.image_view())
            .call();
        let depth_attachment = depth_attachment_info()
            .view(self.depth_image.image_view())
            .call();
        let color_attachments = [color_attachment_info];
        let draw_extent = self.draw_extent();
        let rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: draw_extent,
            })
            .color_attachments(&color_attachments)
            .depth_attachment(&depth_attachment)
            .layer_count(1);
        unsafe { device.cmd_begin_rendering(cmd, &rendering_info) };

        unsafe {
            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.mesh_pipeline.pipeline(),
            );
        };

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: draw_extent.width as f32,
            height: draw_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        unsafe { device.cmd_set_viewport(cmd, 0, &[viewport]) };

        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: draw_extent,
        };
        unsafe { device.cmd_set_scissor(cmd, 0, &[scissor]) };

        let aspect_ratio = draw_extent.width as f32 / draw_extent.height as f32;
        let mut projection =
            Mat4::perspective_rh(f32::to_radians(70.0), aspect_ratio, 10000.0, 0.1);
        projection.y_axis.y *= -1.0;
        let matrix = projection * self.mesh_matrix;
        let susanne = &self.meshes[2];
        let push_constants =
            GPUDrawPushConstants::new(matrix, susanne.mesh_buffers().vertex_buffer_addr());

        unsafe {
            device.cmd_push_constants(
                cmd,
                self.mesh_pipeline.layout(),
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::bytes_of(&push_constants),
            );
        };

        unsafe {
            device.cmd_bind_index_buffer(
                cmd,
                susanne.mesh_buffers().index_buffer().buffer(),
                0,
                vk::IndexType::UINT32,
            );
        };

        unsafe {
            device.cmd_draw_indexed(
                cmd,
                susanne.surfaces()[0].count(),
                1,
                susanne.surfaces()[0].start_index(),
                0,
                0,
            );
        };

        unsafe { device.cmd_end_rendering(cmd) };
    }

    fn resize_swapchain(&mut self) -> eyre::Result<()> {
        self.swapchain
            .destroy(self.vulkan.device(), &self.vulkan.swapchain_device());

        let PhysicalSize { width, height } = self.window.inner_size();

        self.swapchain = Swapchain::new(
            width,
            height,
            &self.vulkan,
            swapchain::IMAGE_FORMAT,
            swapchain::COLOR_SPACE,
            vk::PresentModeKHR::FIFO,
            vk::ImageUsageFlags::TRANSFER_DST,
        )?;
        self.resize_swapchain = false;
        Ok(())
    }
    pub fn render(&mut self, gui: &mut Gui) -> eyre::Result<()> {
        if self.resize_swapchain {
            self.resize_swapchain()?;
        }
        let device = self.vulkan.device();
        unsafe {
            device.wait_for_fences(
                &[self.frames.get_current_frame().render_fence()],
                true,
                u64::MAX,
            )
        }?;
        unsafe { device.reset_fences(&[self.frames.get_current_frame().render_fence()]) }?;
        gui.free_textures()?;

        let (primitives, pixels_per_point) = gui.generate_ui(self)?;

        let swapchain_device = self.vulkan.swapchain_device();

        let image_index = match unsafe {
            swapchain_device.acquire_next_image(
                self.swapchain.swapchain(),
                u64::MAX,
                self.frames.get_current_frame().swapchain_semaphore(),
                vk::Fence::null(),
            )
        } {
            Err(e) if e == vk::Result::ERROR_OUT_OF_DATE_KHR => {
                self.resize_swapchain = true;
                return Ok(());
            }
            Ok((_, true)) => {
                self.resize_swapchain = true;
                return Ok(());
            }
            Ok((i, _)) => i,
            Err(e) => return Err(eyre!("{e}")),
        };

        let cmd = self.frames.get_current_frame().cmd_buffer();
        self.record_commands(gui, &primitives, pixels_per_point, image_index, cmd)?;

        let render_semaphore = self.submit(image_index, cmd)?;

        self.present(&swapchain_device, image_index, render_semaphore)?;

        self.frames.advance();

        Ok(())
    }

    fn record_commands(
        &self,
        gui: &mut Gui,
        primitives: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
        image_index: u32,
        cmd: vk::CommandBuffer,
    ) -> Result<(), eyre::Error> {
        let device = self.vulkan.device();
        unsafe { device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty()) }?;
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(cmd, &begin_info) }?;
        let draw_image = self.draw_image.image();
        transition_image(
            device,
            cmd,
            draw_image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::GENERAL,
        );
        self.draw_background(cmd);
        transition_image(
            device,
            cmd,
            draw_image,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        transition_image(
            device,
            cmd,
            self.depth_image.image(),
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
        );

        self.draw_geometry(cmd);
        transition_image(
            device,
            cmd,
            draw_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let swapchain_image = self.swapchain.images()[image_index as usize];
        transition_image(
            device,
            cmd,
            swapchain_image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let draw_extent = self.draw_extent();
        copy_image_to_image(
            device,
            cmd,
            draw_image,
            swapchain_image,
            draw_extent,
            self.swapchain.extent(),
        );
        transition_image(
            device,
            cmd,
            swapchain_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        let swapchain_image_view = self.swapchain.image_views()[image_index as usize];
        gui.draw_gui(
            device,
            cmd,
            swapchain_image_view,
            self.swapchain.extent(),
            pixels_per_point,
            primitives,
        )?;
        transition_image(
            device,
            cmd,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        unsafe { device.end_command_buffer(cmd) }?;
        Ok(())
    }

    fn submit(
        &self,
        image_index: u32,
        cmd: vk::CommandBuffer,
    ) -> Result<vk::Semaphore, eyre::Error> {
        let device = self.vulkan.device();
        let current_frame = self.frames.get_current_frame();
        let cmd_info = vk::CommandBufferSubmitInfo::default()
            .command_buffer(cmd)
            .device_mask(0);
        let render_semaphore = self.render_semaphores[image_index as usize];
        let wait_info = semaphore_submit_info(
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            current_frame.swapchain_semaphore(),
        );
        let signal_info =
            semaphore_submit_info(vk::PipelineStageFlags2::ALL_GRAPHICS, render_semaphore);
        let wait_infos = [wait_info];
        let signal_infos = [signal_info];
        let cmd_infos = [cmd_info];
        let submit_info = vk::SubmitInfo2::default()
            .wait_semaphore_infos(&wait_infos)
            .signal_semaphore_infos(&signal_infos)
            .command_buffer_infos(&cmd_infos);
        let graphics_queue = self.vulkan.graphics_queue();
        unsafe {
            device.queue_submit2(graphics_queue, &[submit_info], current_frame.render_fence())
        }?;
        Ok(render_semaphore)
    }

    fn present(
        &mut self,
        swapchain_device: &ash::khr::swapchain::Device,
        image_index: u32,
        render_semaphore: vk::Semaphore,
    ) -> Result<(), eyre::Error> {
        let swapchains = [self.swapchain.swapchain()];
        let wait_semaphores = [render_semaphore];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .swapchains(&swapchains)
            .wait_semaphores(&wait_semaphores)
            .image_indices(&image_indices);
        match unsafe { swapchain_device.queue_present(self.vulkan.present_queue(), &present_info) }
        {
            Err(e) if e == vk::Result::ERROR_OUT_OF_DATE_KHR => {
                self.resize_swapchain = true;
                return Ok(());
            }
            Ok(true) => {
                self.resize_swapchain = true;
                return Ok(());
            }
            Ok(_) => (),
            Err(e) => return Err(eyre!("{e}")),
        }
        Ok(())
    }

    pub const fn resize(&mut self, _size: PhysicalSize<u32>) {
        self.resize_swapchain = true;
    }

    pub fn window_event(&mut self, event: &WindowEvent, gui: &mut Gui) {
        let _ = gui.winit_mut().on_window_event(&self.window, event);
        #[allow(clippy::single_match)]
        match event {
            WindowEvent::Occluded(occluded) => self.render = !occluded,
            _ => {}
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub const fn vulkan(&self) -> &Vulkan {
        &self.vulkan
    }

    pub const fn swapchain(&self) -> &Swapchain {
        &self.swapchain
    }

    pub const fn immediate_graphics(&self) -> &ImmediateSubmit {
        &self.immediate_graphics
    }
}
