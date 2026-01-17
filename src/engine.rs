use std::sync::Arc;

use ash::vk;
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{
    swapchain::{self, Swapchain},
    vulkan::Vulkan,
};

pub struct Engine {
    pub window: Arc<Window>,
    pub render: bool,
    vulkan: Vulkan,
    swapchain: Swapchain,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let swapchain_device = self.vulkan.swapchain_device();
        unsafe { swapchain_device.destroy_swapchain(self.swapchain.swapchain(), None) };
        let device = self.vulkan.device();
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
        Ok(Self {
            window: Arc::new(window),
            render: true,
            vulkan,
            swapchain,
        })
    }

    pub fn render(&mut self) {}

    pub fn resize(&mut self, size: PhysicalSize<u32>) {}

    pub fn window_event(&mut self, event: &WindowEvent) {
        #[allow(clippy::single_match)]
        match event {
            WindowEvent::Occluded(occluded) => self.render = !occluded,
            _ => {}
        }
    }
}
