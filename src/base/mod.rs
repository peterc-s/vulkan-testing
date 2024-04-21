use std::{
    ffi::{CString, CStr},
    os::raw::{c_char, c_void},
    cell::RefCell,
    str,
};

use ash::{
    ext::debug_utils,
    vk, Entry, Instance
};

use winit::{
    dpi::LogicalSize,
    error::EventLoopError,
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{EventLoop, ControlFlow},
    keyboard::{Key, NamedKey},
    platform::run_on_demand::EventLoopExtRunOnDemand,
    raw_window_handle::HasDisplayHandle,
    window::WindowBuilder,
};

use crate::util::constants::*;
use crate::util::*;

use anyhow::{anyhow, Result};

use log::*;

pub struct App {
    pub entry: Entry,
    pub instance: Instance,
    pub debug_utils_loader: Option<debug_utils::Instance>,
    pub debug_call_back: Option<vk::DebugUtilsMessengerEXT>,
    pub window: winit::window::Window,
    pub event_loop: RefCell<EventLoop<()>>,
}

impl App {
    pub fn create() -> Result<Self> {
        let event_loop = EventLoop::new()?;

        // create window with set size as per vulkan tutorial
        let window = WindowBuilder::new()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .with_resizable(false)
            .build(&event_loop)
            .unwrap();

        let entry = Entry::linked();

        // validation layer
        let layer_names = [ unsafe { CStr::from_bytes_with_nul_unchecked(
            b"VK_LAYER_KHRONOS_validation\0",
        ) }];

        let layer_names_raw: Vec<*const c_char> = layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();

        let mut extension_names = ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())
                .unwrap()
                .to_vec();

        // add debug utils extension if needed
        if VALIDATION_ENABLED {
            extension_names.push(debug_utils::NAME.as_ptr());
        }

        // macos and ios stuff
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

        let app_name = CString::new(WINDOW_TITLE).unwrap();
        let engine_name = CString::new("Vulkan Engine").unwrap();

        // create struct that holds the applications info
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&engine_name)
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 0, 0));

        // create the struct that holds instance creation info
        let mut create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_names_raw)
            .enabled_extension_names(&extension_names)
            .flags(create_flags);

        // setup debug stuff needed later
        let mut debug_utils_loader: Option<debug_utils::Instance> = None;
        let mut debug_call_back: Option<vk::DebugUtilsMessengerEXT> = None;
        let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                              | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                              | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                              | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE)
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                          | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                          | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE)
            .pfn_user_callback(Some(debug_callback));

        // so we get debugging on creating instance and such
        if VALIDATION_ENABLED {
            create_info = create_info.push_next(&mut debug_info);
        }

        // actually create the instance
        let instance: Instance = unsafe { entry
            .create_instance(&create_info, None)
            .expect("Instance creation failed.")
        };

        // setup the rest of the debugging stuff
        if VALIDATION_ENABLED {
            debug_utils_loader = Some(debug_utils::Instance::new(&entry, &instance));

            debug_call_back = Some(unsafe { debug_utils_loader.as_ref().unwrap()
                                   .create_debug_utils_messenger(&debug_info, None)? });
        }

        // device stuff
        let phys_devices = unsafe { match instance.enumerate_physical_devices() {
            Ok(pdevices) => pdevices,
            Err(e) => return Err(anyhow!("Failed to find GPUs with Vulkan support: {:?}", e))
        } };

        let (phys_device, queue_family_index) = match phys_devices
            .iter()
            .find_map(|pdevice| {
                unsafe {
                    instance
                        .get_physical_device_queue_family_properties(*pdevice)
                        .iter()
                        .enumerate()
                        .find_map(|(index, info)| {
                            let support_graphics = info.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                            if support_graphics {
                                Some((*pdevice, index))
                            } else {
                                None
                            }
                        })
                }
            }) {
                Some(v) => v,
                None => return Err(anyhow!("Failed to find suitable GPU."))
            };

        println!("Chosen device: {:?}", unsafe { string_from_utf8(&instance.get_physical_device_properties(phys_device).device_name) } );

        Ok(Self {
            entry,
            instance,
            debug_utils_loader,
            debug_call_back,
            window,
            event_loop: RefCell::new(event_loop),
        })
    }

    pub fn render_loop<F: Fn()>(&self, f: F) -> Result<(), EventLoopError> {
        self.event_loop.borrow_mut().run_on_demand(|event, elwp| {
            elwp.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested 
                        | WindowEvent::KeyboardInput {
                            event: KeyEvent {
                                state: ElementState::Pressed,
                                logical_key: Key::Named(NamedKey::Escape),
                                ..
                            },
                            ..
                        }, 
                    ..
                } => {
                    println!("Exiting!");
                    elwp.exit();
                },
                Event::AboutToWait => f(),
                _ => {},
            }
        })
    }
}

impl Drop for App {
    // so rust will clean up after us when the app is dropped
    fn drop(&mut self) {
        unsafe {
            match (self.debug_utils_loader.clone(), self.debug_call_back) {
                (Some(loader), Some(call_back)) => loader.destroy_debug_utils_messenger(call_back, None),
                _ => {}
            };

            self.instance.destroy_instance(None);
        }
    }
}

// debug message callback
pub extern "system" fn debug_callback(
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
