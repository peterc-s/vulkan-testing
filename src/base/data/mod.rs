use ash::{ext::debug_utils, khr::{swapchain, surface}, vk::{self, SurfaceKHR, SwapchainKHR}, Device, Instance};

use anyhow::{Result, anyhow};

pub struct DebugData {
    pub utils_loader: debug_utils::Instance,
    pub callback: vk::DebugUtilsMessengerEXT,
}

pub struct SurfaceData {
    pub surface: SurfaceKHR,
    pub loader: surface::Instance,
}

pub struct PhysicalDeviceData {
    pub device: vk::PhysicalDevice,
    pub swapchain_support: SwapchainSupport,
}

pub struct QueueData {
    pub family_indices: QueueFamilyIndices,
    pub present: vk::Queue,
    pub graphics: vk::Queue,
}

impl QueueData {
    pub unsafe fn get(
        queue_family_indices: QueueFamilyIndices,
        logical_device: &Device,
    ) -> Self {
        QueueData {
            family_indices: queue_family_indices,
            present: logical_device.get_device_queue(queue_family_indices.present, 0),
            graphics: logical_device.get_device_queue(queue_family_indices.graphics, 0),
        }
    }
}

pub struct SwapchainData {
    pub swapchain: SwapchainKHR,
    pub loader: swapchain::Device,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
}

pub struct PipelineData {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
}

pub struct SyncObjects {
    pub image_available_semaphores: Vec<vk::Semaphore>,
    pub render_finished_semaphores: Vec<vk::Semaphore>,
    pub in_flight_fences: Vec<vk::Fence>,
    pub images_in_flight: Vec<vk::Fence>,
}

#[derive(Debug, Clone, Default)]
pub struct SwapchainSupport {
    pub capabilities: vk::SurfaceCapabilitiesKHR,
    pub formats: Vec<vk::SurfaceFormatKHR>,
    pub present_modes: Vec<vk::PresentModeKHR>,
}

impl SwapchainSupport {
    pub unsafe fn get(
        surface_data: &SurfaceData,
        physical_device: vk::PhysicalDevice

    ) -> Result<Self> {
        Ok(Self {
            capabilities: surface_data.loader.get_physical_device_surface_capabilities(physical_device, surface_data.surface)?,
            formats: surface_data.loader.get_physical_device_surface_formats(physical_device, surface_data.surface)?,
            present_modes: surface_data.loader.get_physical_device_surface_present_modes(physical_device, surface_data.surface)?,
        })
    }

    pub fn get_surface_format(&self) -> vk::SurfaceFormatKHR {
        self.formats
            .iter()
            .cloned()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_SRGB &&
                f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or_else(|| self.formats[0])
    }

    pub fn get_present_mode(&self) -> vk::PresentModeKHR {
        self.present_modes
            .iter()
            .cloned()
            .find(|m| *m == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO)
    }

    pub fn get_extent(&self, window: &winit::window::Window) -> vk::Extent2D {
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

#[derive(Debug, Clone, Copy, Default)]
pub struct QueueFamilyIndices {
    pub graphics: u32,
    pub present: u32,
}

impl QueueFamilyIndices {
    pub unsafe fn get(instance: &Instance, surface_data: &SurfaceData, phys_device: vk::PhysicalDevice) -> Result<Self> {
        let properties = instance.get_physical_device_queue_family_properties(phys_device);

        let graphics = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        let mut present = None;
        for (index, _properties) in properties.iter().enumerate() {
            if surface_data.loader.get_physical_device_surface_support(phys_device, index as u32, surface_data.surface)? {
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

