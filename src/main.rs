use std::ffi::CString;

use vulkan_testing::util::constants::*;

use winit::{
    dpi::LogicalSize,
    error::EventLoopError,
    event::{ElementState, Event, WindowEvent},
    event_loop::EventLoop,
    keyboard::{Key, NamedKey},
    raw_window_handle::HasDisplayHandle,
    window::WindowBuilder,
};

use ash::{vk, Entry, ext::debug_utils};

struct VulkanApp {
    _entry: ash::Entry,
    instance: ash::Instance,
}

impl VulkanApp {
    pub fn new(window: &winit::window::Window) -> VulkanApp {
        // create linked entry
        let entry = Entry::linked();

        // create instance with window
        let instance = VulkanApp::create_instance(&entry, window);

        VulkanApp {
            _entry: entry,
            instance,
        }
    }

    fn create_instance(entry: &Entry, window: &winit::window::Window) -> ash::Instance {
        // create the VkApplicationInfo struct
        let app_name = CString::new(WINDOW_TITLE).unwrap();
        let app_info = vk::ApplicationInfo {
            p_application_name: app_name.as_ptr(),
            api_version: API_VERSION,
            ..Default::default()
        };

        // get the required extensions using the window display handle
        let mut extension_names = ash_window::enumerate_required_extensions(window.display_handle().unwrap().as_raw())
            .unwrap()
            .to_vec();

        // add debug_utils to required extensions
        extension_names.push(debug_utils::NAME.as_ptr());

        // create the VkInstanceCreateInfo struct
        let create_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            pp_enabled_extension_names: extension_names.as_ptr(),
            enabled_extension_count: extension_names.len() as u32,
            ..Default::default()
        };

        // create the instance itself
        let instance = unsafe { entry.create_instance(&create_info, None).unwrap() };

        instance
    }

    fn init_window(event_loop: &EventLoop<()>) -> winit::window::Window {
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

    pub fn main_loop(self, event_loop: EventLoop<()>) -> Result<(), EventLoopError> {
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
    let window = VulkanApp::init_window(&event_loop);
    let vulkan_app = VulkanApp::new(&window);

    vulkan_app.main_loop(event_loop).expect("Error in main event loop.");
}
