use std::{
    //error::Error,
    ffi::{CString, CStr},
    os::raw::{c_char, c_void},
};

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

use ash::{vk, Entry, ext::debug_utils, prelude::VkResult};

use anyhow::Result;

use log::*;

struct VulkanApp {
    _entry: ash::Entry,
    instance: ash::Instance,
    data: AppData,
}

impl VulkanApp {
    pub fn new(window: &winit::window::Window) -> Result<Self> {
        // create linked entry
        let entry = Entry::linked();

        let mut data = AppData::default();

        // create instance with window
        let instance = VulkanApp::create_instance(&entry, window, &mut data)?;

        Ok(VulkanApp {
            _entry: entry,
            instance,
            data,
        })
    }

    fn create_instance(entry: &Entry, window: &winit::window::Window, data: &mut AppData) -> VkResult<ash::Instance> {
        // create the VkApplicationInfo struct
        let app_name = CString::new(WINDOW_TITLE).unwrap();
        let app_info = vk::ApplicationInfo {
            p_application_name: app_name.as_ptr(),
            api_version: API_VERSION,
            ..Default::default()
        };

        // get the required extensions using the window display handle
        let mut extension_names = ash_window::enumerate_required_extensions(window.display_handle().unwrap().as_raw())?
            .to_vec();

        // add debug_utils to required extensions
        if VALIDATION_ENABLED {
            extension_names.push(debug_utils::NAME.as_ptr());
        }

        // ios & macos stuff
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
            extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
        }

        let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::default()
        };

        // create the VkInstanceCreateInfo struct
        let mut create_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            pp_enabled_extension_names: extension_names.as_ptr(),
            enabled_extension_count: extension_names.len() as u32,
            flags: create_flags,
            ..Default::default()
        };

        // add validation layer if validation enabled
        let layer_names_raw: Vec<*const c_char>;

        let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                              | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                              | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                              | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE)
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                          | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                          | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE)
            .pfn_user_callback(Some(debug_callback));
        
        if VALIDATION_ENABLED {
            let layer_names = [VALIDATION_LAYER];
            layer_names_raw = layer_names
                .iter()
                .map(|raw_name| raw_name.as_ptr())
                .collect();

            create_info = create_info.enabled_layer_names(&layer_names_raw).push_next(&mut debug_info);
        }

        // create the instance itself
        let instance = unsafe { entry.create_instance(&create_info, None).unwrap() };

        if VALIDATION_ENABLED {
            let debug_utils_loader = debug_utils::Instance::new(&entry, &instance);
            data.debug_messenger = unsafe { debug_utils_loader
                .create_debug_utils_messenger(&debug_info, None)
                .unwrap() };
        }

        Ok(instance)
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

#[derive(Debug, Default, Clone)]
struct AppData {
    debug_messenger: vk::DebugUtilsMessengerEXT, 
}

extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    let data = unsafe { *data };
    let message = unsafe { CStr::from_ptr(data.p_message) }.to_string_lossy();

    if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
        error!("({:?}) {}", type_, message);
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
        warn!("({:?}) {}", type_, message);
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::INFO {
        debug!("({:?}) {}", type_, message);
    } else {
        trace!("({:?}) {}", type_, message);
    }

    vk::FALSE
}


fn main() {
    pretty_env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    let window = VulkanApp::init_window(&event_loop);
    let vulkan_app = VulkanApp::new(&window).expect("Failed to create VulkanApp.");

    vulkan_app.main_loop(event_loop).expect("Error in main event loop.");
}
