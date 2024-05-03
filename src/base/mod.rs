use std::{
    ffi::{CString, CStr},
    os::raw::{c_char, c_void},
    collections::HashSet,
};

use ash::{
    ext::debug_utils,
    khr::{surface, swapchain},
    vk::{self, Handle},
    Device, Entry, Instance
};

use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::util::constants::*;
use crate::util::Bytecode;

use anyhow::{anyhow, Result};

use log::*;

use self::data::{PipelineData, SyncObjects};

mod data;

/* 
 * Main structs
 */

// holds all the top-level important data
pub struct App {
    pub entry: Entry,
    pub window: winit::window::Window,
    pub instance: Instance,
    pub debug_data: Option<data::DebugData>,
    pub surface_data: data::SurfaceData,
    pub physical_device_data: data::PhysicalDeviceData,
    pub queue_data: data::QueueData,
    pub logical_device: Device,
    pub swapchain_data: data::SwapchainData,
    pub render_pass: vk::RenderPass,
    pub pipeline_data: data::PipelineData,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub sync_objects: data::SyncObjects,
    pub frame: usize,
}

impl App {
    pub fn create(window: winit::window::Window) -> Result<Self> {
        /* entry */
        info!("Creating entry.");
        let entry = Entry::linked();

        /* instance */
        info!("Creating instance.");
        let instance = create_instance(&window, &entry)?;

        if VALIDATION_ENABLED {
            info!("Creating debug utils loader and callback.")
        }
        let debug_data = create_debug_data(&instance, &entry);

        /* surface */
        info!("Creating surface.");
        let surface_data = create_surface(&entry, &instance, &window)?;

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

        let physical_device_data = choose_device(&instance, &surface_data, &device_extension_names)?;

        let queue_family_indices = unsafe { data::QueueFamilyIndices::get(&instance, &surface_data, physical_device_data.device)? };
        
        info!("Creating logical device.");
        let logical_device = create_logical_device(&instance, &physical_device_data, &queue_family_indices, &device_extension_names_raw)?;

        let queue_data = unsafe { data::QueueData::get(queue_family_indices, &logical_device) };

        info!("Creating swapchain.");
        let swapchain_data = create_swapchain(&window, &instance, &surface_data, &physical_device_data, &queue_data, &logical_device)?;

        info!("Creating render pass.");
        let render_pass = create_render_pass(&logical_device, &swapchain_data)?;

        info!("Creating pipeline.");
        let pipeline_data = create_pipeline(&logical_device, &swapchain_data, &render_pass)?;

        info!("Creating framebuffers.");
        let framebuffers = create_framebuffers(&logical_device, &swapchain_data, &render_pass)?;

        info!("Creating command pool.");
        let command_pool = create_command_pool(&queue_data, &logical_device)?;

        info!("Creating command buffers.");
        let command_buffers = create_command_buffers(&logical_device, &swapchain_data, &render_pass, &pipeline_data.pipeline, &framebuffers, &command_pool)?;

        info!("Creating sync objects.");
        let sync_objects = create_sync_objects(&logical_device, &swapchain_data)?;

        let frame: usize = 0;

        Ok(
            Self {
                entry,
                instance,
                debug_data,
                window,
                surface_data,
                physical_device_data,
                queue_data,
                swapchain_data,
                logical_device,
                render_pass,
                pipeline_data,
                framebuffers,
                command_pool,
                command_buffers,
                sync_objects,
                frame,
            }
        )
    }

    pub unsafe fn render_frame(
        &mut self,
    ) -> Result<()> {
        let in_flight_fence = self.sync_objects.in_flight_fences[self.frame];
        self.logical_device.wait_for_fences(&[in_flight_fence], true, u64::MAX)?;

        let image_index = self.swapchain_data
            .loader
            .acquire_next_image(
                self.swapchain_data.swapchain,
                u64::MAX,
                self.sync_objects.image_available_semaphores[self.frame],
                vk::Fence::null(),
            )?
            .0 as usize;

        let image_in_flight = self.sync_objects.images_in_flight[image_index];
        if !image_in_flight.is_null() {
            self.logical_device.wait_for_fences(&[image_in_flight], true, u64::MAX)?;
        }

        self.sync_objects.images_in_flight[image_index] = in_flight_fence;

        let wait_semaphores = &[self.sync_objects.image_available_semaphores[self.frame]];
        let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = &[self.command_buffers[image_index]];
        let signal_semaphores = &[self.sync_objects.render_finished_semaphores[self.frame]];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        self.logical_device.reset_fences(&[in_flight_fence])?;

        self.logical_device.queue_submit(self.queue_data.graphics, &[submit_info], self.sync_objects.in_flight_fences[self.frame])?;

        let swapchains = &[self.swapchain_data.swapchain];
        let image_indices = &[image_index as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        self.swapchain_data.loader.queue_present(self.queue_data.present, &present_info)?;

        self.frame = (self.frame + 1) & MAX_FRAMES_IN_FLIGHT;

        Ok(())
    }

    pub unsafe fn destroy(&mut self) {
        self.logical_device.device_wait_idle().unwrap();

        self.sync_objects.in_flight_fences.iter().for_each(|f| self.logical_device.destroy_fence(*f, None));
        self.sync_objects.render_finished_semaphores.iter().for_each(|s| self.logical_device.destroy_semaphore(*s, None));
        self.sync_objects.image_available_semaphores.iter().for_each(|s| self.logical_device.destroy_semaphore(*s, None));
        self.logical_device.destroy_command_pool(self.command_pool, None);
        self.framebuffers.iter().for_each(|f| self.logical_device.destroy_framebuffer(*f, None));
        self.logical_device.destroy_pipeline(self.pipeline_data.pipeline, None);
        self.logical_device.destroy_pipeline_layout(self.pipeline_data.layout, None);
        self.logical_device.destroy_render_pass(self.render_pass, None);
        self.swapchain_data.image_views.iter().for_each(|v| self.logical_device.destroy_image_view(*v, None));
        self.swapchain_data.loader.destroy_swapchain(self.swapchain_data.swapchain, None);
        self.logical_device.destroy_device(None);
        self.surface_data.loader.destroy_surface(self.surface_data.surface, None);

        if VALIDATION_ENABLED {
            self.debug_data
                .as_ref()
                .unwrap()
                .utils_loader
                .clone()
                .destroy_debug_utils_messenger(self.debug_data.as_ref().unwrap().callback, None);
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

    Ok(instance)
}

fn create_debug_data (
    instance: &Instance,
    entry: &Entry,
    ) -> Option<data::DebugData> {
    // setup debug create info
    let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                          | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                          | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                          | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE)
        .message_type(vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                      | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                      | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE)
        .pfn_user_callback(Some(debug_callback));

    let mut debug_data: Option<data::DebugData> = None;

    if VALIDATION_ENABLED {
        let utils_loader = debug_utils::Instance::new(entry, &instance);
        let callback = unsafe { utils_loader.create_debug_utils_messenger(&debug_info, None).expect("Failed to create debug callback.") };

        debug_data = Some( data::DebugData {
            utils_loader,
            callback,
        });
    }

    debug_data
}

fn create_surface(
        entry: &Entry,
        instance: &Instance,
        window: &winit::window::Window,
    ) -> Result<data::SurfaceData> {

    let surface = unsafe {
        ash_window::create_surface(
            entry,
            instance,
            window.display_handle()?.as_raw(),
            window.window_handle()?.as_raw(),
            None,
        )?
    };


    let loader = surface::Instance::new(&entry, &instance);

    Ok(
        data::SurfaceData {
            surface,
            loader,
        }
    )
}

fn choose_device(
        instance: &Instance,
        surface_data: &data::SurfaceData,
        device_extension_names: &Vec<&CStr>,
    ) -> Result<data::PhysicalDeviceData> {
    // check if any vulkan supported GPUs exist
    info!("Enumerating physical devices.");
    let phys_devices = unsafe { match instance.enumerate_physical_devices() {
        Ok(pdevices) => pdevices,
        Err(e) => return Err(anyhow!("Failed to find GPUs with Vulkan support: {:?}", e))
    } };

    let mut phys_device = Err(());
    let mut swapchain_support = Err(());
    
    // iterate through devices
    for pdevice in phys_devices {
        // check for required queue families
        if let Ok(_) = unsafe { data::QueueFamilyIndices::get(instance, surface_data, pdevice) } {
            // get the devices extension properties
            let extensions = unsafe {
                instance.enumerate_device_extension_properties(pdevice)?
                    .iter()
                    .map(|e| CStr::from_ptr(e.extension_name.as_ptr()))
                    .collect::<Vec<_>>()
            };

            // check for needed extensions
            if !device_extension_names
                    .iter()
                    .all(|e| extensions.contains(e)) {
                break;
            }

            swapchain_support = Ok(
                unsafe {
                    data::SwapchainSupport::get(surface_data, pdevice)?
                }
            );

            if swapchain_support.as_ref().unwrap().formats.is_empty() ||
               swapchain_support.as_ref().unwrap().present_modes.is_empty() {
                break;
            }

            phys_device = Ok(pdevice);

        }
    }

    match (phys_device, swapchain_support) {
        (Ok(device), Ok(swapchain_support)) => Ok(
            data::PhysicalDeviceData {
                device,
                swapchain_support,
            }
        ),
        _ => Err(anyhow!("Failed to find suitable device.")),
    }
}

fn create_logical_device(
        instance: &Instance,
        physical_device_data: &data::PhysicalDeviceData,
        queue_family_indices: &data::QueueFamilyIndices,
        device_extension_names_raw: &Vec<*const i8>,
    ) -> Result<Device> {
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
        match instance.create_device(physical_device_data.device, &device_create_info, None) {
            Ok(d) => d,
            Err(e) => return Err(anyhow!("Failed to create logical device: {:?}", e))
        }
    };

    Ok(device)
}

fn create_swapchain(
        window: &winit::window::Window,
        instance: &Instance,
        surface_data: &data::SurfaceData,
        physical_device_data: &data::PhysicalDeviceData,
        queue_data: &data::QueueData,
        device: &Device,
    ) -> Result<data::SwapchainData> {
    let swapchain_surface_format = physical_device_data.swapchain_support.get_surface_format();
    let format = swapchain_surface_format.format;
    let swapchain_present_mode = physical_device_data.swapchain_support.get_present_mode();
    let extent = physical_device_data.swapchain_support.get_extent(window);

    let mut swapchain_image_count = physical_device_data.swapchain_support.capabilities.min_image_count + 1;
    if physical_device_data.swapchain_support.capabilities.max_image_count != 0
        && swapchain_image_count > physical_device_data.swapchain_support.capabilities.max_image_count 
    {
            swapchain_image_count = physical_device_data.swapchain_support.capabilities.max_image_count;
    }

    let mut swapchain_qf_indices = vec![];
    let image_sharing_mode = if queue_data.family_indices.graphics != queue_data.family_indices.present {
        swapchain_qf_indices.push(queue_data.family_indices.graphics);
        swapchain_qf_indices.push(queue_data.family_indices.present);
        vk::SharingMode::CONCURRENT
    } else {
        vk::SharingMode::EXCLUSIVE
    };

    let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface_data.surface)
        .min_image_count(swapchain_image_count)
        .image_format(swapchain_surface_format.format)
        .image_color_space(swapchain_surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(image_sharing_mode)
        .queue_family_indices(&swapchain_qf_indices)
        .pre_transform(physical_device_data.swapchain_support.capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(swapchain_present_mode)
        .clipped(true)
        .old_swapchain(vk::SwapchainKHR::null());

    let loader = swapchain::Device::new(instance, device);

    let swapchain = unsafe { loader
        .create_swapchain(&swapchain_create_info, None)
        .unwrap() };

    let images = unsafe { loader.get_swapchain_images(swapchain)? };

    let image_views = create_swapchain_image_views(&images, &format, device)?;

    Ok(
        data::SwapchainData {
            swapchain,
            loader,
            format,
            extent,
            images,
            image_views,
        }
    )
}

fn create_swapchain_image_views(
        swapchain_images: &Vec<vk::Image>,
        swapchain_format: &vk::Format,
        device: &Device,
    ) -> Result<Vec<vk::ImageView>> {
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

    Ok(
        swapchain_images
            .iter()
            .map(|i| {
                let info = vk::ImageViewCreateInfo::default()
                    .image(*i)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(*swapchain_format)
                    .components(components)
                    .subresource_range(subresource_range);

                unsafe { device.create_image_view(&info, None) }
            })
            .collect::<Result<Vec<_>, _>>()?
    )
}

fn create_render_pass(
    device: &Device,
    swapchain_data: &data::SwapchainData,
) -> Result<vk::RenderPass> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(swapchain_data.format)
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

    unsafe { Ok(device.create_render_pass(&info, None)?) }
}

fn create_pipeline(
    device: &Device,
    swapchain_data: &data::SwapchainData,
    render_pass: &vk::RenderPass,
) -> Result<PipelineData> {
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
        .width(swapchain_data.extent.width as f32)
        .height(swapchain_data.extent.height as f32)
        .min_depth(0.0)
        .max_depth(0.0);

    let scissor = vk::Rect2D::default()
        .offset(vk::Offset2D { x: 0, y: 0})
        .extent(swapchain_data.extent);

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

    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None)? };

    let stages = &[vert_stage, frag_stage];
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(stages)
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .layout(pipeline_layout)
        .render_pass(*render_pass)
        .subpass(0);

    let pipeline = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None).unwrap()[0] };

    unsafe {
        device.destroy_shader_module(vert_shader_module, None);
        device.destroy_shader_module(frag_shader_module, None);
    }

    Ok(
        data::PipelineData {
            pipeline,
            layout: pipeline_layout,
        }
    )
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
    swapchain_data: &data::SwapchainData,
    render_pass: &vk::RenderPass,
) -> Result<Vec<vk::Framebuffer>> {
    Ok(swapchain_data.image_views
        .iter()
        .map(|i| {
            let attachments = &[*i];
            let framebuffer_create_info = vk::FramebufferCreateInfo::default()
                .render_pass(*render_pass)
                .attachments(attachments)
                .width(swapchain_data.extent.width)
                .height(swapchain_data.extent.height)
                .layers(1);

            unsafe { device.create_framebuffer(&framebuffer_create_info, None) }
        }).collect::<Result<Vec<_>, _>>()?
    )
}

fn create_command_pool(
    queue_data: &data::QueueData,
    device: &Device,
) -> Result<vk::CommandPool> {
    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_data.family_indices.graphics);

    unsafe { Ok(device.create_command_pool(&command_pool_info, None)?) }
}

fn create_command_buffers(
    device: &Device,
    swapchain_data: &data::SwapchainData,
    render_pass: &vk::RenderPass,
    pipeline: &vk::Pipeline,
    framebuffers: &Vec<vk::Framebuffer>,
    command_pool: &vk::CommandPool,
) -> Result<Vec<vk::CommandBuffer>> {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(*command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(framebuffers.len() as u32);

    let command_buffers = unsafe { device.allocate_command_buffers(&allocate_info)? };

    for (i, command_buffer) in command_buffers.iter().enumerate() {
        let begin_info = vk::CommandBufferBeginInfo::default();

        unsafe { device.begin_command_buffer(*command_buffer, &begin_info)? };

        let render_area = vk::Rect2D::default()
            .offset(vk::Offset2D::default())
            .extent(swapchain_data.extent);

        let color_clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        };

        let clear_values = &[color_clear_value];
        let pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(*render_pass)
            .framebuffer(framebuffers[i])
            .render_area(render_area)
            .clear_values(clear_values);

        unsafe {
            device.cmd_begin_render_pass(*command_buffer, &pass_begin_info, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(*command_buffer, vk::PipelineBindPoint::GRAPHICS, *pipeline);
            device.cmd_draw(*command_buffer, 3, 1, 0, 0);
            device.cmd_end_render_pass(*command_buffer);
            device.end_command_buffer(*command_buffer)?;
        };
    }

    Ok(command_buffers)
}

fn create_sync_objects(
    device: &Device,
    swapchain_data: &data::SwapchainData,
) -> Result<SyncObjects> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default()
        .flags(vk::FenceCreateFlags::SIGNALED);

    let mut image_available_semaphores = vec![];
    let mut render_finished_semaphores = vec![];
    let mut in_flight_fences = vec![];
    

    unsafe {
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            image_available_semaphores.push(device.create_semaphore(&semaphore_info, None)?);
            render_finished_semaphores.push(device.create_semaphore(&semaphore_info, None)?);
            in_flight_fences.push(device.create_fence(&fence_info, None)?);
        }
    }

    let images_in_flight = swapchain_data.images
        .iter()
        .map(|_| vk::Fence::null())
        .collect();

    Ok(
        data::SyncObjects {
            image_available_semaphores,
            render_finished_semaphores,
            in_flight_fences,
            images_in_flight,
        }
    )
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
