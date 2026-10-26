//! Zstandard: with the zstd C library with the `libzstd` feature, in native builds; otherwise
//! with the pure-Rust `ruzstd`, which compresses at its fastest level only.

use std::io::Result;

#[cfg(all(feature = "libzstd", not(target_family = "wasm")))]
pub(crate) fn compress(data: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::bulk::compress(data, level)
}

#[cfg(not(all(feature = "libzstd", not(target_family = "wasm"))))]
pub(crate) fn compress(data: &[u8], _level: i32) -> Result<Vec<u8>> {
    Ok(ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest))
}

/// Decompresses a whole stream.
#[cfg(all(feature = "libzstd", not(target_family = "wasm")))]
#[cfg_attr(any(target_family = "wasm", not(feature = "pointcloud")), allow(dead_code))]
pub(crate) fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(data)
}

/// Decompresses a whole stream.
#[cfg(not(all(feature = "libzstd", not(target_family = "wasm"))))]
#[cfg_attr(any(target_family = "wasm", not(feature = "pointcloud")), allow(dead_code))]
pub(crate) fn decompress(mut data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut d = ruzstd::decoding::StreamingDecoder::new(&mut data).map_err(std::io::Error::other)?;
    std::io::Read::read_to_end(&mut d, &mut out)?;
    Ok(out)
}
