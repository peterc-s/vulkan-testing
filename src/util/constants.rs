use ash::vk;
use std::ffi::CStr;

pub const API_VERSION: u32 = vk::make_api_version(0,1,0,0);

pub const WINDOW_TITLE: &'static str = "Vulkan Testing";
pub const WINDOW_HEIGHT: u32 = 600;
pub const WINDOW_WIDTH: u32 = 800;

pub const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
pub const VALIDATION_LAYER: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") };
