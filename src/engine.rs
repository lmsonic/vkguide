use std::{mem::ManuallyDrop, sync::Arc};

use ash::vk;
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{
    descriptors::{DescriptorAllocator, PoolSizeRatio},
    frames::Frames,
    gui::{Gui, GuiApp},
    immediate::ImmediateSubmit,
    pipeline::BackgroundPipeline,
    shader::ShaderCompiler,
    swapchain::{self, Swapchain},
    texture::{DrawImage, copy_image_to_image},
    utils::{semaphore_submit_info, transition_image},
    vulkan::Vulkan,
};

pub struct Engine {
    pub window: Arc<Window>,
    pub render: bool,
    vulkan: Vulkan,
    allocator: ManuallyDrop<vk_mem::Allocator>,
    swapchain: Swapchain,
    frames: Frames,
    shader_compiler: ShaderCompiler,
    descriptor_allocator: DescriptorAllocator,
    draw_image: DrawImage,
    background_pipeline: BackgroundPipeline,
    immediate_submit: ImmediateSubmit,
    gui: ManuallyDrop<Gui>,
}

impl Engine {
    pub fn destroy(&mut self, egui_app: &mut impl GuiApp) {
        unsafe { self.vulkan.device().device_wait_idle() }.unwrap();
        egui_app.destroy(self);
        let device = self.vulkan.device();

        self.frames.destroy(device);
        //
        unsafe { ManuallyDrop::drop(&mut self.gui) };
        self.immediate_submit.destroy(device);
        self.background_pipeline.destroy(device);
        self.descriptor_allocator.destroy_pool(device);
        self.draw_image.destroy(device, &self.allocator);

        unsafe { ManuallyDrop::drop(&mut self.allocator) };
        //

        let swapchain_device = self.vulkan.swapchain_device();
        self.swapchain.destroy(device, &swapchain_device);

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
    pub fn new(window: Window) -> eyre::Result<Self> {
        let vulkan = Vulkan::new(&window)?;
        let swapchain = Swapchain::new(
            &window,
            &vulkan,
            swapchain::IMAGE_FORMAT,
            swapchain::COLOR_SPACE,
            vk::PresentModeKHR::FIFO,
            vk::ImageUsageFlags::TRANSFER_DST,
        )?;
        let frames = Frames::new(&vulkan)?;

        let device = vulkan.device();
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
        let draw_image = DrawImage::new(&window, device, &allocator, &descriptor_allocator)?;
        let background_pipeline = BackgroundPipeline::new(device, &shader_compiler, &draw_image)?;
        let immediate_submit = ImmediateSubmit::new(device, vulkan.queue_family_indices())?;
        let gui = Gui::new(&window, &vulkan, &swapchain)?;
        Ok(Self {
            window: Arc::new(window),
            render: true,
            vulkan,
            swapchain,
            frames,
            allocator: ManuallyDrop::new(allocator),
            draw_image,
            shader_compiler,
            descriptor_allocator,
            background_pipeline,
            immediate_submit,
            gui: ManuallyDrop::new(gui),
        })
    }

    fn draw_background(&self, cmd: vk::CommandBuffer) {
        let device = self.vulkan.device();

        unsafe {
            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.background_pipeline.pipeline(),
            );
        };
        unsafe {
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.background_pipeline.layout(),
                0,
                &[self.draw_image.descriptor_set()],
                &[],
            );
        };

        let draw_extent = vk::Extent2D {
            width: self.draw_image.extent().width,
            height: self.draw_image.extent().height,
        };
        unsafe {
            device.cmd_dispatch(
                cmd,
                draw_extent.width.div_ceil(16),
                draw_extent.height.div_ceil(16),
                1,
            );
        };
    }

    pub fn render(&mut self, gui_app: &mut impl GuiApp) -> eyre::Result<()> {
        let device = &self.vulkan.device().clone();
        let current_frame = self.frames.get_current_frame();
        unsafe { device.wait_for_fences(&[current_frame.render_fence()], true, u64::MAX) }?;
        unsafe { device.reset_fences(&[current_frame.render_fence()]) }?;
        self.gui.free_textures()?;

        let (primitives, pixels_per_point) =
            self.gui
                .generate_ui(gui_app, &self.window, &self.vulkan, &self.immediate_submit)?;

        let swapchain_device = self.vulkan.swapchain_device();
        let (image_index, _) = unsafe {
            swapchain_device.acquire_next_image(
                self.swapchain.swapchain(),
                u64::MAX,
                current_frame.swapchain_semaphore(),
                vk::Fence::null(),
            )
        }?;

        let cmd = current_frame.cmd_buffer();
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

        let draw_extent = vk::Extent2D {
            width: self.draw_image.extent().width,
            height: self.draw_image.extent().height,
        };
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
        self.gui.draw_gui(
            device,
            cmd,
            swapchain_image_view,
            self.swapchain.extent(),
            pixels_per_point,
            &primitives,
        )?;
        transition_image(
            device,
            cmd,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        unsafe { device.end_command_buffer(cmd) }?;

        let cmd_info = vk::CommandBufferSubmitInfo::default()
            .command_buffer(cmd)
            .device_mask(0);

        let render_semaphore = self.swapchain.render_semaphores()[image_index as usize];
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

        let swapchains = [self.swapchain.swapchain()];
        let wait_semaphores = [render_semaphore];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .swapchains(&swapchains)
            .wait_semaphores(&wait_semaphores)
            .image_indices(&image_indices);

        unsafe { swapchain_device.queue_present(self.vulkan.present_queue(), &present_info) }?;

        self.frames.advance();

        Ok(())
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {}

    pub fn window_event(&mut self, event: &WindowEvent) {
        #[allow(clippy::single_match)]
        match event {
            WindowEvent::Occluded(occluded) => self.render = !occluded,
            _ => {}
        }
    }
}
