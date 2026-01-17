use std::sync::Arc;

use ash::vk;
use eyre::Ok;
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{
    frames::{FRAMES_IN_FLIGHT, FrameData, create_frames},
    swapchain::{self, Swapchain},
    utils::{image_subresource_range, semaphore_submit_info, transition_image},
    vulkan::Vulkan,
};

pub struct Engine {
    pub window: Arc<Window>,
    pub render: bool,
    vulkan: Vulkan,
    swapchain: Swapchain,
    frames: [FrameData; FRAMES_IN_FLIGHT],
    frame_index: usize,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let device = self.vulkan.device();
        unsafe { device.device_wait_idle() }.unwrap();
        for f in self.frames {
            unsafe { device.destroy_command_pool(f.cmd_pool(), None) };
            unsafe { device.destroy_fence(f.fence(), None) };
            unsafe { device.destroy_semaphore(f.swapchain_semaphore(), None) };
            unsafe { device.destroy_semaphore(f.render_semaphore(), None) };
        }

        let swapchain_device = self.vulkan.swapchain_device();
        unsafe { swapchain_device.destroy_swapchain(self.swapchain.swapchain(), None) };
        for v in self.swapchain.image_views() {
            unsafe { device.destroy_image_view(*v, None) };
        }
        let debug_instance = self.vulkan.debug_instance();
        unsafe { device.destroy_device(None) };
        let surface_instance = self.vulkan.surface_instance();
        unsafe { surface_instance.destroy_surface(self.vulkan.surface(), None) };
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
        Ok(Self {
            window: Arc::new(window),
            render: true,
            vulkan,
            swapchain,
            frames,
            frame_index: 0,
        })
    }

    const fn get_current_frame(&self) -> &FrameData {
        &self.frames[self.frame_index % self.frames.len()]
    }

    pub fn render(&mut self) -> eyre::Result<()> {
        let current_frame = self.get_current_frame();
        let device = self.vulkan.device();
        unsafe { device.wait_for_fences(&[current_frame.fence()], true, u64::MAX) }?;
        unsafe { device.reset_fences(&[current_frame.fence()]) }?;

        let swapchain_device = self.vulkan.swapchain_device();
        let (image_index, code) = unsafe {
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
        let image = self.swapchain.images()[image_index as usize];
        transition_image(
            device,
            cmd,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::GENERAL,
        );
        let flash = (self.frame_index as f32 / 120.0).sin().abs();
        let clear_value = vk::ClearColorValue {
            float32: [0.0, 0.0, flash, 1.0],
        };
        let subresource_range = image_subresource_range(vk::ImageAspectFlags::COLOR);
        unsafe {
            device.cmd_clear_color_image(
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                &clear_value,
                &[subresource_range],
            );
        };
        unsafe { device.end_command_buffer(cmd) }?;
        let wait_info = semaphore_submit_info(
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            current_frame.swapchain_semaphore(),
        );
        let signal_info = semaphore_submit_info(
            vk::PipelineStageFlags2::ALL_GRAPHICS,
            current_frame.render_semaphore(),
        );
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
