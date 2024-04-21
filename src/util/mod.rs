pub mod constants;

pub unsafe fn string_from_utf8(string: &[i8; 256]) -> String {
    std::str::from_utf8_unchecked(&string.iter()
                                  .filter(|&i| *i as u8 != b'\0')
                                  .map(|&i| i as u8)
                                  .collect::<Vec<_>>()).to_string()
}
