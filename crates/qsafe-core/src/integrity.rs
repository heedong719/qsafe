use crate::error::Result;

pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

pub fn verify_blake3(data: &[u8], expected: &[u8; 32]) -> Result<()> {
    let actual = blake3_hash(data);
    if &actual == expected {
        Ok(())
    } else {
        Err(crate::error::CoreError::IntegrityFailed)
    }
}
