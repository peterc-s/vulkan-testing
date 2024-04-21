use winit::{
    dpi::LogicalSize,
    error::EventLoopError,
    event::{ElementState, Event, WindowEvent},
    event_loop::EventLoop,
    keyboard::{Key, NamedKey},
    window::{Window, WindowBuilder}
};

// Consts
const WINDOW_TITLE: &'static str = "Vulkan Testing";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

struct VulkanApp;

impl VulkanApp {
    fn init_window(event_loop: &EventLoop<()>) -> Window {
        // following the vulkan tutorial,
        // creates a window with the given title,
        // the size in the vulkan tutorial,
        // and unresizable as in the tutorial.
        WindowBuilder::new()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .with_resizable(false)
            .build(event_loop)
            .expect("Failed to create window.")
    }

    pub fn main_loop(event_loop: EventLoop<()>) -> Result<(), EventLoopError> {
        // runs the main event loop
        // `event` is the given event,
        // `elwt` is the event loop window target that
        // allows for control flow.
        event_loop.run(move |event, elwt| {
            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    println!("Exiting!");
                    elwt.exit();
                },

                Event::WindowEvent {
                    event: WindowEvent::KeyboardInput { event, .. },
                    ..
                } => {
                    match event.logical_key {
                        Key::Named(named_key) => {
                            match (named_key, event.state) {
                                (NamedKey::Escape, ElementState::Pressed) => {
                                    println!("Escape pressed, exiting!");
                                    elwt.exit();
                                }
                                _ => {},
                            }
                        },
                        _ => {},
                    }
                },

                _ => {},
            }
        })
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let _window = VulkanApp::init_window(&event_loop);

    let _ = VulkanApp::main_loop(event_loop);
}
