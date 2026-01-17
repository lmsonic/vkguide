use std::time::Duration;

use winit::{
    application::ApplicationHandler, dpi::LogicalSize, event::WindowEvent,
    event_loop::ActiveEventLoop, window::Window,
};

use crate::engine::Engine;

pub struct App {
    pub(crate) engine: Option<Engine>,
}

impl App {
    #[must_use]
    pub const fn new() -> Self {
        Self { engine: None }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler for App {
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

        self.engine = Some(Engine::new(window).expect("could not create engine"));
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
                self.engine.take();
            }

            WindowEvent::RedrawRequested => {
                if let Some(engine) = &mut self.engine {
                    if engine.render {
                        engine.render().expect("could not render");
                        engine.window.request_redraw();
                    } else {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }

            event => {
                if let Some(engine) = &mut self.engine {
                    engine.window_event(&event);
                }
            }
        }
    }
}
