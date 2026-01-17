use time::macros::format_description;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, time::LocalTime},
    layer::SubscriberExt,
    util::SubscriberInitExt as _,
};
use vkguide::app::App;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    let timer = LocalTime::new(format_description!(
        version = 2,
        "[hour]:[minute]:[second]:[subsecond]"
    ));
    tracing_subscriber::registry()
        .with(fmt::layer().compact().with_timer(timer))
        .with(EnvFilter::from_default_env())
        .init();
    color_eyre::install().unwrap();
    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
