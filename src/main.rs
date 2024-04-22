use vulkan_testing::{ base::App, util::constants::* };

use winit::{
    dpi::LogicalSize,
    window::WindowBuilder,
    event_loop::EventLoop,
};

use anyhow::Result;

use log::*;
use std::process;

fn main() -> Result<()> {
    pretty_env_logger::init();

    let event_loop = EventLoop::new()?;

    // create window with set size as per vulkan tutorial
    let window = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
        .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    let app = match App::create(window, event_loop) {
        Ok(a) => a,
        Err(e) => {
            error!("Error creating app: {:?}", e);
            process::exit(1)
        }
    };

    let _ = app.render_loop(|| {});

    Ok(())
}
