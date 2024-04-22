use std::{
    ffi::{CString, CStr},
    os::raw::{c_char, c_void},
    collections::HashSet,
};

use ash::{
    ext::debug_utils, khr::{surface, swapchain}, vk::{self, Handle, SurfaceKHR}, Device, Entry, Instance
};

use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
};

use crate::util::constants::*;
use crate::util::{string_from_utf8, Bytecode};

use anyhow::{anyhow, Result};

use log::*;

/* 
 * Main structs
 */

// holds all the top-level important data
pub struct App {
    pub data: AppData,
    pub entry: Entry,
    pub window: winit::window::Window,
    pub instance: Instance,
    pub surface_loader: surface::Instance,
    pub device: Device,
    pub swapchain_loader: swapchain::Device,
    pub frame: usize,
}

// holds all the lower level data
#[derive(Default)]
pub struct AppData {
    pub debug_utils_loader: Option<debug_utils::Instance>,
    pub debug_call_back: Option<vk::DebugUtilsMessengerEXT>,
    pub phys_device: vk::PhysicalDevice,
    pub queue_family_indices: QueueFamilyIndices,
    pub present_queue: vk::Queue,
    pub graphics_queue: vk::Queue,
    pub surface: vk::SurfaceKHR,
    pub swapchain_support: SwapchainSupport,
    pub swapchain_format: vk::Format,
    pub swapchain_extent: vk::Extent2D,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub pipeline_layout: vk::PipelineLayout,
    pub render_pass: vk::RenderPass,
    pub pipeline: vk::Pipeline,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub image_available_semaphores: Vec<vk::Semaphore>,
    pub render_finished_semaphores: Vec<vk::Semaphore>,
    pub in_flight_fences: Vec<vk::Fence>,
    pub images_in_flight: Vec<vk::Fence>,
}

impl App {
    pub fn create(window: winit::window::Window) -> Result<Self> {
        let mut data = AppData::default();

        /* entry */
        info!("Creating entry.");
        let entry = Entry::linked();

        /* instance */
        info!("Creating instance.");
        let instance = create_instance(&window, &entry, &mut data)?;

        /* surface */
        info!("Creating surface.");
        let surface_loader = create_surface(&entry, &instance, &window, &mut data)?;

        /* physical device */
        info!("Choosing device.");
        // get required device extension names
        let device_extension_names = vec![
            swapchain::NAME,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ash::khr::portability_subset::NAME,
        ];

        // get required device extension names as pointers
        let device_extension_names_raw = device_extension_names.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();

        choose_device(&instance, &surface_loader, &device_extension_names, &mut data)?;

        /* logical device */
        info!("Creating logical device.");
        let device = create_logical_device(&instance, &surface_loader, &device_extension_names_raw, &mut data)?;

        // get a handle to the device queues
        data.present_queue = unsafe { device.get_device_queue(data.queue_family_indices.present, 0) };
        data.graphics_queue = unsafe { device.get_device_queue(data.queue_family_indices.graphics, 0) };

        /* swapchain */
        info!("Creating swapchain.");
        let swapchain_loader = create_swapchain(&window, &instance, &device, &mut data)?;

        /* swapchain image views */
        info!("Creating swapchain image views.");
        create_swapchain_image_views(&device, &mut data)?;

        /* render pass*/
        info!("Creating render pass.");
        create_render_pass(&device, &mut data)?;

        /* pipeline */
        info!("Creating pipeline.");
        create_pipeline(&device, &mut data)?;

        /* framebuffers */
        info!("Creating framebuffers.");
        create_framebuffers(&device, &mut data)?;

        /* command buffers */
        info!("Creating command pool.");
        create_command_pool(&device, &mut data)?;
        info!("Creating command buffers.");
        create_command_buffers(&device, &mut data)?;

        /* semaphores & fence */
        info!("Creating sync objects.");
        create_sync_objects(&device, &mut data)?;

        let frame: usize = 0;

        Ok(Self {
            data,
            entry,
            instance,
            window,
            device,
            surface_loader,
            swapchain_loader,
            frame,
        })
    }

    pub unsafe fn render_frame(
        &mut self,
    ) -> Result<()> {
        let in_flight_fence = self.data.in_flight_fences[self.frame];
        self.device.wait_for_fences(&[in_flight_fence], true, u64::MAX)?;

        let image_index = self.swapchain_loader
            .acquire_next_image(
                self.data.swapchain,
                u64::MAX,
                self.data.image_available_semaphores[self.frame],
                vk::Fence::null(),
            )?
            .0 as usize;

        let image_in_flight = self.data.images_in_flight[image_index];
        if !image_in_flight.is_null() {
            self.device.wait_for_fences(&[image_in_flight], true, u64::MAX)?;
        }

        self.data.images_in_flight[image_index] = in_flight_fence;

        let wait_semaphores = &[self.data.image_available_semaphores[self.frame]];
        let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = &[self.data.command_buffers[image_index]];
        let signal_semaphores = &[self.data.render_finished_semaphores[self.frame]];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        self.device.reset_fences(&[in_flight_fence])?;

        self.device.queue_submit(self.data.graphics_queue, &[submit_info], self.data.in_flight_fences[self.frame])?;

        let swapchains = &[self.data.swapchain];
        let image_indices = &[image_index as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        self.swapchain_loader.queue_present(self.data.present_queue, &present_info)?;

        self.frame = (self.frame + 1) & MAX_FRAMES_IN_FLIGHT;

        Ok(())
    }

    pub unsafe fn destroy(&mut self) {
        self.device.device_wait_idle().unwrap();

        self.data.in_flight_fences.iter().for_each(|f| self.device.destroy_fence(*f, None));
        self.data.render_finished_semaphores.iter().for_each(|s| self.device.destroy_semaphore(*s, None));
        self.data.image_available_semaphores.iter().for_each(|s| self.device.destroy_semaphore(*s, None));
        self.device.destroy_command_pool(self.data.command_pool, None);
        self.data.framebuffers.iter().for_each(|f| self.device.destroy_framebuffer(*f, None));
        self.device.destroy_pipeline(self.data.pipeline, None);
        self.device.destroy_pipeline_layout(self.data.pipeline_layout, None);
        self.device.destroy_render_pass(self.data.render_pass, None);
        self.data.swapchain_image_views.iter().for_each(|v| self.device.destroy_image_view(*v, None));
        self.swapchain_loader.destroy_swapchain(self.data.swapchain, None);
        self.device.destroy_device(None);
        self.surface_loader.destroy_surface(self.data.surface, None);

        if VALIDATION_ENABLED {
            self.data.debug_utils_loader.clone().unwrap().destroy_debug_utils_messenger(self.data.debug_call_back.unwrap(), None);
        }

        self.instance.destroy_instance(None);
    }
}

/*
 * Functions
 */

fn create_instance(
        window: &winit::window::Window,
        entry: &Entry,
        data: &mut AppData
    ) -> Result<Instance> {
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
    data.debug_utils_loader = None;
    data.debug_call_back = None;
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
        data.debug_utils_loader = Some(debug_utils::Instance::new(entry, &instance));

        data.debug_call_back = Some(unsafe { data.debug_utils_loader.as_ref().unwrap()
                               .create_debug_utils_messenger(&debug_info, None)? });
    }

    Ok(instance)
}

fn create_surface(
        entry: &Entry,
        instance: &Instance,
        window: &winit::window::Window,
        data: &mut AppData,
    ) -> Result<surface::Instance> {
    // create a surface
    data.surface = match unsafe { ash_window::create_surface(
        entry,
        instance,
        window.display_handle()?.as_raw(),
        window.window_handle()?.as_raw(),
        None) } {
        Ok(s) => s,
        Err(e) => return Err(anyhow!("Failed to create window surface: {:?}", e)),
    };

    let surface_loader = surface::Instance::new(&entry, &instance);

    Ok(surface_loader)
}

fn choose_device(
        instance: &Instance,
        surface_loader: &surface::Instance,
        device_extension_names: &Vec<&CStr>,
        data: &mut AppData,
    ) -> Result<()> {
    // check if any vulkan supported GPUs exist
    info!("Enumerating physical devices.");
    let phys_devices = unsafe { match instance.enumerate_physical_devices() {
        Ok(pdevices) => pdevices,
        Err(e) => return Err(anyhow!("Failed to find GPUs with Vulkan support: {:?}", e))
    } };

    // find a suitable GPU
    let mut phys_device = Err(anyhow!("Failed to find suitable physical device."));
    let mut swapchain_support = Err(anyhow!("Failed to find suitable physical device."));

    info!("Beginning physical device checks.");
    // iterate over all physical devices on system
    for pdevice in phys_devices {
        // get the properties of each device
        let properties = unsafe { instance.get_physical_device_properties(pdevice) };
        info!("Checking physical device (`{}`)", unsafe {string_from_utf8(&properties.device_name)});

        // check for required queue families
        if let Err(error) = unsafe { QueueFamilyIndices::get(instance, surface_loader, pdevice, data) } {
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
            swapchain_support = Ok(unsafe { SwapchainSupport::get(&data.surface, surface_loader, pdevice)? });

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

    data.swapchain_support = match swapchain_support {
        Ok(s) => s,
        Err(err) => return Err(err),
    };

    data.phys_device = match phys_device {
        Ok(p) => p,
        Err(err) => return Err(err),
    };

    println!("Chosen device: {:?}", unsafe { string_from_utf8(&instance.get_physical_device_properties(data.phys_device).device_name) } );

    Ok(())
}

fn create_logical_device(
        instance: &Instance,
        surface_loader: &surface::Instance,
        device_extension_names_raw: &Vec<*const i8>,
        data: &mut AppData
    ) -> Result<Device> {
    data.queue_family_indices = unsafe { QueueFamilyIndices::get(instance, surface_loader, data.phys_device, data)? };

    let mut unique_indices = HashSet::new();
    unique_indices.insert(data.queue_family_indices.graphics);
    unique_indices.insert(data.queue_family_indices.present);

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
        match instance.create_device(data.phys_device, &device_create_info, None) {
            Ok(d) => d,
            Err(e) => return Err(anyhow!("Failed to create logical device: {:?}", e))
        }
    };

    Ok(device)
}

fn create_swapchain(
        window: &winit::window::Window,
        instance: &Instance,
        device: &Device,
        data: &mut AppData,
    ) -> Result<swapchain::Device> {
    let swapchain_surface_format = data.swapchain_support.get_surface_format();
    data.swapchain_format = swapchain_surface_format.format;
    let swapchain_present_mode = data.swapchain_support.get_present_mode();
    data.swapchain_extent = data.swapchain_support.get_extent(window);

    let mut swapchain_image_count = data.swapchain_support.capabilities.min_image_count + 1;
    if data.swapchain_support.capabilities.max_image_count != 0
        && swapchain_image_count > data.swapchain_support.capabilities.max_image_count 
    {
            swapchain_image_count = data.swapchain_support.capabilities.max_image_count;
    }

    let mut swapchain_qf_indices = vec![];
    let image_sharing_mode = if data.queue_family_indices.graphics != data.queue_family_indices.present {
        swapchain_qf_indices.push(data.queue_family_indices.graphics);
        swapchain_qf_indices.push(data.queue_family_indices.present);
        vk::SharingMode::CONCURRENT
    } else {
        vk::SharingMode::EXCLUSIVE
    };

    let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(data.surface)
        .min_image_count(swapchain_image_count)
        .image_format(swapchain_surface_format.format)
        .image_color_space(swapchain_surface_format.color_space)
        .image_extent(data.swapchain_extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(image_sharing_mode)
        .queue_family_indices(&swapchain_qf_indices)
        .pre_transform(data.swapchain_support.capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(swapchain_present_mode)
        .clipped(true)
        .old_swapchain(vk::SwapchainKHR::null());

    let swapchain_loader = swapchain::Device::new(instance, device);

    data.swapchain = unsafe { swapchain_loader
        .create_swapchain(&swapchain_create_info, None)
        .unwrap() };

    data.swapchain_images = unsafe { swapchain_loader.get_swapchain_images(data.swapchain)? };

    Ok(swapchain_loader)
}

fn create_swapchain_image_views(
        device: &Device,
        data: &mut AppData
    ) -> Result<()> {
    let components = vk::ComponentMapping::default()
        .r(vk::ComponentSwizzle::IDENTITY)
        .g(vk::ComponentSwizzle::IDENTITY)
        .b(vk::ComponentSwizzle::IDENTITY)
        .a(vk::ComponentSwizzle::IDENTITY);

    let subresource_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);

    data.swapchain_image_views = data.swapchain_images
        .iter()
        .map(|i| {
            let info = vk::ImageViewCreateInfo::default()
                .image(*i)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(data.swapchain_format)
                .components(components)
                .subresource_range(subresource_range);

            unsafe { device.create_image_view(&info, None) }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(())
}

fn create_render_pass(
    //instance: &Instance,
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(data.swapchain_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

    let color_attachment_ref = vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

    let color_attachments = &[color_attachment_ref];
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(color_attachments);

    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

    let attachments = &[color_attachment];
    let subpasses = &[subpass];
    let dependencies = &[dependency];
    let info = vk::RenderPassCreateInfo::default()
        .attachments(attachments)
        .subpasses(subpasses)
        .dependencies(dependencies);

    data.render_pass = unsafe { device.create_render_pass(&info, None)? };

    Ok(())
}

fn create_pipeline(
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    let vert = include_bytes!("../shaders/vert.spv");
    let frag = include_bytes!("../shaders/frag.spv");

    let vert_shader_module = create_shader_module(&device, vert)?;
    let frag_shader_module = create_shader_module(&device, frag)?;

    let vert_stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::VERTEX)
        .module(vert_shader_module)
        .name(SHADER_MAIN);

    let frag_stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(frag_shader_module)
        .name(SHADER_MAIN);

    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();

    let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    // for future reference
    // let dynamic_states = [
    //     vk::DynamicState::VIEWPORT,
    //     vk::DynamicState::SCISSOR,
    // ];
    //
    // let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
    //     .dynamic_states(&dynamic_states);

    let viewport = vk::Viewport::default()
        .x(0.0)
        .y(0.0)
        .width(data.swapchain_extent.width as f32)
        .height(data.swapchain_extent.height as f32)
        .min_depth(0.0)
        .max_depth(0.0);

    let scissor = vk::Rect2D::default()
        .offset(vk::Offset2D { x: 0, y: 0})
        .extent(data.swapchain_extent);

    let viewports = &[viewport];
    let scissors = &[scissor];
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewports(viewports)
        .scissors(scissors);

    let rasterizer_state = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_bias_enable(true)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::CLOCKWISE)
        .depth_bias_enable(false);

    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment_state = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);

    let color_blend_attachments = [color_blend_attachment_state];

    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default();

    data.pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None)? };

    let stages = &[vert_stage, frag_stage];
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(stages)
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .layout(data.pipeline_layout)
        .render_pass(data.render_pass)
        .subpass(0);

    data.pipeline = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None).unwrap()[0] };

    unsafe {
        device.destroy_shader_module(vert_shader_module, None);
        device.destroy_shader_module(frag_shader_module, None);
    }

    Ok(())
}

fn create_shader_module(
    device: &Device,
    bytecode: &[u8],
) -> Result<vk::ShaderModule> {
    let bytecode = Bytecode::from(bytecode)?;

    let info = vk::ShaderModuleCreateInfo::default()
        .code(bytecode.code());

    let shader_module = unsafe { device.create_shader_module(&info, None)? };

    Ok(shader_module)
}

fn create_framebuffers(
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    data.framebuffers = data.swapchain_image_views
        .iter()
        .map(|i| {
            let attachments = &[*i];
            let framebuffer_create_info = vk::FramebufferCreateInfo::default()
                .render_pass(data.render_pass)
                .attachments(attachments)
                .width(data.swapchain_extent.width)
                .height(data.swapchain_extent.height)
                .layers(1);

            unsafe { device.create_framebuffer(&framebuffer_create_info, None) }
        }).collect::<Result<Vec<_>, _>>()?;
    
    Ok(())
}

fn create_command_pool(
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(data.queue_family_indices.graphics);

    data.command_pool = unsafe { device.create_command_pool(&command_pool_info, None)? };

    Ok(())
}

fn create_command_buffers(
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(data.command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(data.framebuffers.len() as u32);

    data.command_buffers = unsafe { device.allocate_command_buffers(&allocate_info)? };

    for (i, command_buffer) in data.command_buffers.iter().enumerate() {
        let begin_info = vk::CommandBufferBeginInfo::default();

        unsafe { device.begin_command_buffer(*command_buffer, &begin_info)? };

        let render_area = vk::Rect2D::default()
            .offset(vk::Offset2D::default())
            .extent(data.swapchain_extent);

        let color_clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        };

        let clear_values = &[color_clear_value];
        let pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(data.render_pass)
            .framebuffer(data.framebuffers[i])
            .render_area(render_area)
            .clear_values(clear_values);

        unsafe {
            device.cmd_begin_render_pass(*command_buffer, &pass_begin_info, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(*command_buffer, vk::PipelineBindPoint::GRAPHICS, data.pipeline);
            device.cmd_draw(*command_buffer, 3, 1, 0, 0);
            device.cmd_end_render_pass(*command_buffer);
            device.end_command_buffer(*command_buffer)?;
        };
    }

    Ok(())
}

fn create_sync_objects(
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default()
        .flags(vk::FenceCreateFlags::SIGNALED);

    unsafe {
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            data.image_available_semaphores.push(device.create_semaphore(&semaphore_info, None)?);
            data.render_finished_semaphores.push(device.create_semaphore(&semaphore_info, None)?);
            data.in_flight_fences.push(device.create_fence(&fence_info, None)?);
        }
    }

    data.images_in_flight = data.swapchain_images
        .iter()
        .map(|_| vk::Fence::null())
        .collect();

    Ok(())
}

/*
 * Structs
 */

#[derive(Debug, Clone, Copy, Default)]
pub struct QueueFamilyIndices {
    graphics: u32,
    present: u32,
}

impl QueueFamilyIndices {
    unsafe fn get(instance: &Instance, surface_loader: &surface::Instance, phys_device: vk::PhysicalDevice, data: &mut AppData) -> Result<Self> {
        info!("Getting queue family indices.");
        let properties = instance.get_physical_device_queue_family_properties(phys_device);

        let graphics = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        let mut present = None;
        for (index, _properties) in properties.iter().enumerate() {
            if surface_loader.get_physical_device_surface_support(phys_device, index as u32, data.surface)? {
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

#[derive(Debug, Clone, Default)]
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

/*
 * Other
 */

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
