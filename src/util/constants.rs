use std::ffi::CStr;

pub const WINDOW_TITLE: &'static str = "Vulkan Testing";
pub const WINDOW_HEIGHT: u32 = 600;
pub const WINDOW_WIDTH: u32 = 800;

pub const VALIDATION_ENABLED: bool = cfg!(debug_assertions);

pub const SHADER_MAIN: &CStr =  unsafe { &CStr::from_bytes_with_nul_unchecked(b"main\0") };
