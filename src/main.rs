use vulkan_testing::{ base::App, util::constants::* };

use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{EventLoop, ControlFlow},
    keyboard::{Key, NamedKey},
    platform::run_on_demand::EventLoopExtRunOnDemand,
    window::WindowBuilder,
};

use anyhow::Result;

use log::*;
use std::process;

fn main() -> Result<()> {
    pretty_env_logger::init();

    let mut event_loop = EventLoop::new()?;

    // create window with set size as per vulkan tutorial
    let window = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
        .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    let mut app = match App::create(window) {
        Ok(a) => a,
        Err(e) => {
            error!("Error creating app: {:?}", e);
            process::exit(1)
        }
    };

    event_loop.run_on_demand(|event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);
        match event {
            Event::AboutToWait => app.window.request_redraw(),
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::RedrawRequested if !elwt.exiting() => unsafe { app.render_frame() }.unwrap(),
                    WindowEvent::KeyboardInput { event: KeyEvent {
                            logical_key: Key::Named(NamedKey::Escape),
                            state: ElementState::Pressed,
                            ..
                        }, ..
                    }
                    | WindowEvent::CloseRequested => {
                        elwt.exit();
                        unsafe { app.destroy() };
                    },
                    _ => {},
                }
            },
            _ => {},
        }
    })?;

    Ok(())
}
