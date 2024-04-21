use ash::vk;

pub const API_VERSION: u32 = vk::make_api_version(0,1,0,0);

pub const WINDOW_TITLE: &'static str = "Vulkan Testing";
pub const WINDOW_HEIGHT: u32 = 600;
pub const WINDOW_WIDTH: u32 = 800;
