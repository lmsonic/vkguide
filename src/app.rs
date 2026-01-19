use std::{mem::ManuallyDrop, sync::Arc, time::Duration};

use winit::{
    application::ApplicationHandler, dpi::LogicalSize, event::WindowEvent,
    event_loop::ActiveEventLoop, window::Window,
};

use crate::{engine::Engine, gui::Gui};

pub struct AppWrapper {
    pub(crate) engine: Option<Engine>,
    pub(crate) gui: Option<ManuallyDrop<Gui>>,
}

impl AppWrapper {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            engine: None,
            gui: None,
        }
    }
}

impl Default for AppWrapper {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler for AppWrapper {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        const WINDOW_WIDTH: u32 = 1080;
        const WINDOW_HEIGHT: u32 = 720;

        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Vulkan Engine")
                    .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
            )
            .expect("could not create window");
        window.request_redraw();
        let window = Arc::new(window);
        let engine = Engine::new(Arc::clone(&window)).expect("could not create engine");
        let gui = ManuallyDrop::new(
            Gui::new(&window, engine.vulkan(), engine.swapchain()).expect("could not create gui"),
        );
        self.gui = Some(gui);
        self.engine = Some(engine);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(mut engine) = self.engine.take()
            && let Some(mut gui) = self.gui.take()
        {
            engine.destroy(&mut gui);
        }
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                if let Some(engine) = &mut self.engine {
                    engine.resize(size);
                }
            }

            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                if let Some(engine) = &mut self.engine
                    && let Some(gui) = &mut self.gui
                {
                    if engine.render {
                        engine.render(gui).expect("could not render");
                        engine.window().request_redraw();
                    } else {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }

            event => {
                if let Some(engine) = &mut self.engine
                    && let Some(gui) = &mut self.gui
                {
                    engine.window_event(&event, gui);
                }
            }
        }
    }
}
