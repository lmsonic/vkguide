use time::macros::format_description;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, time::LocalTime},
    layer::SubscriberExt,
    util::SubscriberInitExt as _,
};
use vkguide::app::DefaultAppWrapper;
use winit::{
    event_loop::{ControlFlow, EventLoop},
    platform::run_on_demand::EventLoopExtRunOnDemand,
};

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
    let mut event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = DefaultAppWrapper::new();
    event_loop.run_app_on_demand(&mut app).unwrap();
}
