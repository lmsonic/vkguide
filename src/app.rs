use std::time::Duration;

use winit::{
    application::ApplicationHandler, dpi::LogicalSize, event::WindowEvent,
    event_loop::ActiveEventLoop, window::Window,
};

use crate::{engine::Engine, gui::GuiApp};

pub struct DefaultGuiApp;

impl GuiApp for DefaultGuiApp {
    fn new(engine: &mut Engine) -> eyre::Result<Self>
    where
        Self: std::marker::Sized,
    {
        Ok(Self {})
    }

    fn build_ui(&mut self, _ctx: &egui::Context) {}
}
pub type DefaultAppWrapper = AppWrapper<DefaultGuiApp>;

pub struct AppWrapper<A: GuiApp> {
    pub(crate) engine: Option<Engine>,
    pub(crate) gui_app: Option<A>,
}

impl<A: GuiApp> AppWrapper<A> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            engine: None,
            gui_app: None,
        }
    }
}

impl<A: GuiApp> Default for AppWrapper<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: GuiApp> ApplicationHandler for AppWrapper<A> {
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
        let mut engine = Engine::new(window).expect("could not create engine");
        self.gui_app = Some(GuiApp::new(&mut engine).expect("could not create gui app"));
        self.engine = Some(engine);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(engine) = &mut self.engine
            && let Some(gui_app) = &mut self.gui_app
        {
            engine.destroy(gui_app);
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
                    && let Some(gui_app) = &mut self.gui_app
                {
                    if engine.render {
                        engine.render(gui_app).expect("could not render");
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
