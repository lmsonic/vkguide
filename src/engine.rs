use std::sync::Arc;

use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::vulkan::Vulkan;

pub struct Engine {
    pub window: Arc<Window>,
    pub render: bool,
}

impl Drop for Engine {
    fn drop(&mut self) {}
}

impl Engine {
    pub fn new(window: Window) -> Self {
        let vulkan = Vulkan::new(&window);
        Self {
            window: Arc::new(window),
            render: true,
        }
    }

    pub fn render(&mut self) {}

    pub fn resize(&mut self, size: PhysicalSize<u32>) {}

    pub fn window_event(&mut self, event: WindowEvent) {
        #[allow(clippy::single_match)]
        match event {
            WindowEvent::Occluded(occluded) => self.render = !occluded,
            _ => {}
        }
    }
}
