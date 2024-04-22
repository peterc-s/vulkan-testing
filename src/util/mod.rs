pub mod constants;

use core::slice;

use anyhow::{Result, anyhow};
use std::alloc::{alloc, dealloc, Layout};

pub unsafe fn string_from_utf8(string: &[i8; 256]) -> String {
    std::str::from_utf8_unchecked(&string.iter()
                                  .filter(|&i| *i as u8 != b'\0')
                                  .map(|&i| i as u8)
                                  .collect::<Vec<_>>()).to_string()
}



// Copyright 2024 Kyle Mayes
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.

// the below Bytecode struct is modified by me, sourced from https://github.com/KyleMayes/vulkanalia/blob/master/vulkanalia/src/bytecode.rs.

#[derive(Debug)]
pub struct Bytecode(*mut u8, usize);

impl Bytecode {
    pub fn from(bytecode: &[u8]) -> Result<Self> {
        if bytecode.is_empty() || bytecode.len() % 4 != 0 {
            return Err(anyhow!("Invalid bytecode buffer length ({})", bytecode.len()));
        }

        let layout = Layout::from_size_align(bytecode.len(), 4)?;
        debug_assert_ne!(layout.size(), 0);

        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return Err(anyhow!("Failed to allocate bytecode buffer."))
        }
        
        let slice = unsafe { slice::from_raw_parts_mut(ptr, layout.size()) };
        slice.copy_from_slice(bytecode);

        Ok(Self(ptr, layout.size()))
    }

    pub fn code(&self) -> &[u32] {
        let ptr: *const u32 = self.0.cast();
        let len = self.1 / 4;
        unsafe { slice::from_raw_parts(ptr, len) }
    }
}

impl Drop for Bytecode {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.1, 4).unwrap();
        unsafe { dealloc(self.0, layout) };
    }
}
