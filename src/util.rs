pub fn u16_from_bytes(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes.try_into().expect("slice must be 2 bytes long"))
}

pub fn u32_from_bytes(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("slice must be 4 bytes long"))
}
