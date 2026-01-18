use std::{mem::ManuallyDrop, sync::Arc};

use ash::vk;
use eyre::Ok;
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{
    frames::{FRAMES_IN_FLIGHT, FrameData, create_frames},
    swapchain::{self, Swapchain},
    texture::{AllocatedImage, copy_image_to_image},
    utils::{image_subresource_range, semaphore_submit_info, transition_image},
    vulkan::Vulkan,
};

pub struct Engine {
    pub window: Arc<Window>,
    pub render: bool,
    vulkan: Vulkan,
    allocator: ManuallyDrop<vk_mem::Allocator>,
    swapchain: Swapchain,
    frames: [FrameData; FRAMES_IN_FLIGHT],
    frame_index: usize,
    draw_image: AllocatedImage,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let device = self.vulkan.device();
        unsafe { device.device_wait_idle() }.unwrap();
        for f in &mut self.frames {
            f.destroy(device);
        }

        // Delete allocate by vma
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
}

impl Engine {
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
        let frames = create_frames(&vulkan)?;

        let mut allocator_info = vk_mem::AllocatorCreateInfo::new(
            vulkan.instance(),
            vulkan.device(),
            vulkan.physical_device(),
        );
        allocator_info.flags = vk_mem::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;
        let allocator = unsafe { vk_mem::Allocator::new(allocator_info) }?;
        let draw_image = AllocatedImage::new(&window, &vulkan, &allocator)?;
        Ok(Self {
            window: Arc::new(window),
            render: true,
            vulkan,
            swapchain,
            frames,
            frame_index: 0,
            allocator: ManuallyDrop::new(allocator),
            draw_image,
        })
    }

    const fn get_current_frame(&self) -> &FrameData {
        &self.frames[self.frame_index % FRAMES_IN_FLIGHT]
    }
    const fn get_current_frame_mut(&mut self) -> &mut FrameData {
        &mut self.frames[self.frame_index % FRAMES_IN_FLIGHT]
    }

    fn draw_background(&self, cmd: vk::CommandBuffer) {
        let device = self.vulkan.device();

        let flash = (self.frame_index as f32 / 120.0).sin().abs();
        let clear_value = vk::ClearColorValue {
            float32: [0.0, 0.0, flash, 1.0],
        };
        let subresource_range = image_subresource_range(vk::ImageAspectFlags::COLOR);
        unsafe {
            device.cmd_clear_color_image(
                cmd,
                self.draw_image.image(),
                vk::ImageLayout::GENERAL,
                &clear_value,
                &[subresource_range],
            );
        };
    }

    pub fn render(&mut self) -> eyre::Result<()> {
        let device = self.vulkan.device();
        let current_frame = self.get_current_frame();
        unsafe { device.wait_for_fences(&[current_frame.render_fence()], true, u64::MAX) }?;

        unsafe { device.reset_fences(&[current_frame.render_fence()]) }?;

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

        self.frame_index += 1;

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
