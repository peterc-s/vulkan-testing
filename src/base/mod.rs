use std::{
    ffi::{CString, CStr},
    os::raw::{c_char, c_void},
    cell::RefCell,
    collections::HashSet,
};

use ash::{
    ext::debug_utils, khr::{surface, swapchain}, vk::{self, SurfaceKHR}, Device, Entry, Instance
};

use winit::{
    dpi::LogicalSize,
    error::EventLoopError,
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    platform::run_on_demand::EventLoopExtRunOnDemand,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
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
    pub device: Device,
    pub phys_device: vk::PhysicalDevice,
    pub queue_family_indices: QueueFamilyIndices,
    pub present_queue: vk::Queue,
    pub graphics_queue: vk::Queue,
    pub surface: vk::SurfaceKHR,
    pub surface_loader: surface::Instance,
    pub swapchain_support: SwapchainSupport,
    pub swapchain_format: vk::Format,
    pub swapchain_extent: vk::Extent2D,
    pub swapchain_loader: swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
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

        // create a surface
        let surface = match unsafe { ash_window::create_surface(
            &entry,
            &instance,
            window.display_handle()?.as_raw(),
            window.window_handle()?.as_raw(),
            None) } {
            Ok(s) => s,
            Err(e) => return Err(anyhow!("Failed to create window surface: {:?}", e)),
        };

        let surface_loader = surface::Instance::new(&entry, &instance);

        // device stuff
        // check if any vulkan supported GPUs exist
        let phys_devices = unsafe { match instance.enumerate_physical_devices() {
            Ok(pdevices) => pdevices,
            Err(e) => return Err(anyhow!("Failed to find GPUs with Vulkan support: {:?}", e))
        } };

        // get required device extension names
        let device_extension_names = [
            swapchain::NAME,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ash::khr::portability_subset::NAME,
        ];

        // get required revice extension names as pointers
        let device_extension_names_raw = device_extension_names.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();

        // find a suitable GPU
        let mut phys_device = Err(anyhow!("Failed to find suitable physical device."));
        let mut swapchain_support = Err(anyhow!("Failed to find suitable physical device."));

        // iterate over all physical devices on system
        for pdevice in phys_devices {
            // get the properties of each device
            let properties = unsafe { instance.get_physical_device_properties(pdevice) };

            // check for required queue families
            if let Err(error) = unsafe { QueueFamilyIndices::get(&instance, &surface, &surface_loader, pdevice) } {
                warn!("Skipping physical device (`{}`): {}", unsafe { string_from_utf8(&properties.device_name) }, error);
            } else {
                // if required queue families are present, then check required device extensions
                info!("Checking physical device (`{}`).", unsafe { string_from_utf8(&properties.device_name) });

                // get the devices extensions
                let extensions = unsafe { instance
                    .enumerate_device_extension_properties(pdevice)?
                    .iter()
                    .map(|e| CStr::from_ptr(e.extension_name.as_ptr()))
                    .collect::<Vec<_>>() };


                // check if all the required extensions are present
                if !device_extension_names.iter().all(|e| extensions.contains(e)) {
                    warn!("Physical device missing required device extension (`{}`).", unsafe { string_from_utf8(&properties.device_name) });
                    break;
                }

                // now that we know our device supports swapchains,
                // we get the swapchain support of the current device
                swapchain_support = Ok(unsafe { SwapchainSupport::get(&surface, &surface_loader, pdevice)? });

                // check if has sufficient swapchain support
                if swapchain_support.as_ref().unwrap().formats.is_empty() || swapchain_support.as_ref().unwrap().present_modes.is_empty() {
                    warn!("Physical device missing sufficient swapchain support (`{}`).", unsafe { string_from_utf8(&properties.device_name) });
                    break;
                }

                info!("Selecting physical device (`{}`).", unsafe { string_from_utf8(&properties.device_name) });
                phys_device = Ok(pdevice);
                break;
            }
        };

        let swapchain_support = match swapchain_support {
            Ok(s) => s,
            Err(err) => return Err(err),
        };

        let phys_device = match phys_device {
            Ok(p) => p,
            Err(err) => return Err(err),
        };

        println!("Chosen device: {:?}", unsafe { string_from_utf8(&instance.get_physical_device_properties(phys_device).device_name) } );

        let queue_family_indices = unsafe { QueueFamilyIndices::get(&instance, &surface, &surface_loader, phys_device)? };

        let mut unique_indices = HashSet::new();
        unique_indices.insert(queue_family_indices.graphics);
        unique_indices.insert(queue_family_indices.present);

        let queue_priorities = [1.0];

        let queue_infos = unique_indices
            .iter()
            .map(|i| {
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(*i)
                    .queue_priorities(&queue_priorities)
            })
            .collect::<Vec<_>>();


        let features = vk::PhysicalDeviceFeatures::default();

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(&device_extension_names_raw)
            .enabled_features(&features);

        // create logical device
        let device: Device = unsafe {
            match instance.create_device(phys_device, &device_create_info, None) {
                Ok(d) => d,
                Err(e) => return Err(anyhow!("Failed to create logical device: {:?}", e))
            }
        };

        // get a handle to the device queue
        let present_queue = unsafe { device.get_device_queue(queue_family_indices.present, 0) };
        let graphics_queue = unsafe { device.get_device_queue(queue_family_indices.graphics, 0) };

        let swapchain_surface_format = swapchain_support.get_surface_format();
        let swapchain_format = swapchain_surface_format.format;
        let swapchain_present_mode = swapchain_support.get_present_mode();
        let swapchain_extent = swapchain_support.get_extent(&window);

        let mut swapchain_image_count = swapchain_support.capabilities.min_image_count + 1;
        if swapchain_support.capabilities.max_image_count != 0
            && swapchain_image_count > swapchain_support.capabilities.max_image_count 
        {
                swapchain_image_count = swapchain_support.capabilities.max_image_count;
        }

        let mut swapchain_qf_indices = vec![];
        let image_sharing_mode = if queue_family_indices.graphics != queue_family_indices.present {
            swapchain_qf_indices.push(queue_family_indices.graphics);
            swapchain_qf_indices.push(queue_family_indices.present);
            vk::SharingMode::CONCURRENT
        } else {
            vk::SharingMode::EXCLUSIVE
        };

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(swapchain_image_count)
            .image_format(swapchain_surface_format.format)
            .image_color_space(swapchain_surface_format.color_space)
            .image_extent(swapchain_extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(image_sharing_mode)
            .queue_family_indices(&swapchain_qf_indices)
            .pre_transform(swapchain_support.capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(swapchain_present_mode)
            .clipped(true)
            .old_swapchain(vk::SwapchainKHR::null());

        let swapchain_loader = swapchain::Device::new(&instance, &device);

        let swapchain = unsafe { swapchain_loader
            .create_swapchain(&swapchain_create_info, None)
            .unwrap() };

        let swapchain_images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };

        Ok(Self {
            entry,
            instance,
            debug_utils_loader,
            debug_call_back,
            window,
            device,
            phys_device,
            queue_family_indices,
            present_queue,
            graphics_queue,
            surface,
            surface_loader,
            swapchain_support,
            swapchain_format,
            swapchain_extent,
            swapchain_loader,
            swapchain,
            swapchain_images,
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
            self.device.device_wait_idle().unwrap();
            
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);

            self.device.destroy_device(None);

            match (self.debug_utils_loader.clone(), self.debug_call_back) {
                (Some(loader), Some(call_back)) => loader.destroy_debug_utils_messenger(call_back, None),
                _ => {}
            };

            self.surface_loader.destroy_surface(self.surface, None);

            self.instance.destroy_instance(None);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QueueFamilyIndices {
    graphics: u32,
    present: u32,
}

impl QueueFamilyIndices {
    unsafe fn get(instance: &Instance, surface: &SurfaceKHR, surface_loader: &surface::Instance, physical_device: vk::PhysicalDevice) -> Result<Self> {
        let properties = instance.get_physical_device_queue_family_properties(physical_device);

        let graphics = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        let mut present = None;
        for (index, _properties) in properties.iter().enumerate() {
            if surface_loader.get_physical_device_surface_support(physical_device, index as u32, *surface)? {
                present = Some(index as u32);
                break;
            }
        }

        if let (Some(graphics), Some(present)) = (graphics, present) {
            Ok(Self { graphics, present })
        } else {
            Err(anyhow!("Missing required queue families."))
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwapchainSupport {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}

impl SwapchainSupport {
    unsafe fn get(
        surface: &SurfaceKHR,
        surface_loader: &surface::Instance,
        physical_device: vk::PhysicalDevice

    ) -> Result<Self> {
        Ok(Self {
            capabilities: surface_loader.get_physical_device_surface_capabilities(physical_device, *surface)?,
            formats: surface_loader.get_physical_device_surface_formats(physical_device, *surface)?,
            present_modes: surface_loader.get_physical_device_surface_present_modes(physical_device, *surface)?,
        })
    }

    fn get_surface_format(&self) -> vk::SurfaceFormatKHR {
        self.formats
            .iter()
            .cloned()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_SRGB &&
                f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or_else(|| self.formats[0])
    }

    fn get_present_mode(&self) -> vk::PresentModeKHR {
        self.present_modes
            .iter()
            .cloned()
            .find(|m| *m == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO)
    }

    fn get_extent(&self, window: &winit::window::Window) -> vk::Extent2D {
        if self.capabilities.current_extent.width != u32::MAX {
            self.capabilities.current_extent
        } else {
            vk::Extent2D::default()
                .width(window.inner_size().width.clamp(
                        self.capabilities.min_image_extent.width,
                        self.capabilities.max_image_extent.width,
                ))
                .height(window.inner_size().height.clamp(
                        self.capabilities.min_image_extent.height,
                        self.capabilities.max_image_extent.height,
                ))
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
