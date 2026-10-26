//! Decoding limits of lazy GeoTIFF reads: a header declaring a chunk far larger than the file is
//! an error, not an allocation that aborts the process.

use terrio::sparse::{SparseFile, read_with};
use terrio::{LazyRaster, TiffReadLimits, Window, WindowOptions};

/// A one-band Float64 TIFF of `width` x `height` pixels in one uncompressed strip of `stored`
/// bytes (zeros) declaring `declared` bytes, little-endian, classic or BigTIFF.
fn tiff(width: u32, height: u32, declared: u32, stored: usize, big: bool) -> Vec<u8> {
    // (tag, type, value): SHORT 3, LONG 4.
    let entries: [(u16, u16, u32); 9] = [
        (256, 4, width),
        (257, 4, height),
        (258, 3, 64),
        (259, 3, 1),
        (262, 3, 1),
        (273, 4, 0), // StripOffsets, set below
        (278, 4, height),
        (279, 4, declared),
        (339, 3, 3),
    ];
    let (header, entry, count, next) = if big { (16, 20, 8, 8) } else { (8, 12, 2, 4) };
    let data_offset = header + count + entries.len() * entry + next;
    let mut out = Vec::new();
    if big {
        out.extend_from_slice(b"II+\0");
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&16u64.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    } else {
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    }
    for (tag, typ, value) in entries {
        let value = if tag == 273 { data_offset as u32 } else { value };
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&typ.to_le_bytes());
        if big {
            out.extend_from_slice(&1u64.to_le_bytes());
            let v = if typ == 3 { u64::from(value as u16) } else { u64::from(value) };
            out.extend_from_slice(&v.to_le_bytes());
        } else {
            out.extend_from_slice(&1u32.to_le_bytes());
            let v = if typ == 3 { u32::from(value as u16) } else { value };
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&vec![0u8; next]);
    assert_eq!(out.len(), data_offset);
    out.resize(data_offset + stored, 0);
    out
}

fn single_strip(width: u32, height: u32, stored: u32, big: bool) -> Vec<u8> {
    tiff(width, height, stored, stored as usize, big)
}

fn band1() -> WindowOptions {
    WindowOptions { bands: vec![1], overview: None }
}

#[test]
fn a_huge_declared_strip_is_an_error() {
    // 10^6 x 10^6 Float64 pixels (8 TB) in one strip of 128 bytes: a 4 x 4 window decodes the
    // strip, which allocated it whole.
    for big in [false, true] {
        let bytes = single_strip(1_000_000, 1_000_000, 128, big);
        let mut r = LazyRaster::from_tiff_bytes(bytes).unwrap().unwrap();
        assert_eq!((r.width(), r.height()), (1_000_000, 1_000_000));
        let e = r.read_window(Window::new(0, 0, 4, 4), &band1()).unwrap_err();
        assert!(e.to_string().contains("decoding limits"), "{e}");
    }
}

#[test]
fn local_and_sparse_files_have_the_limits_too() {
    let bytes = single_strip(1_000_000, 1_000_000, 128, false);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.tif");
    std::fs::write(&path, &bytes).unwrap();
    let mut r = terrio::open_raster_lazy(path.to_str().unwrap()).unwrap();
    assert!(r.read_window(Window::new(0, 0, 4, 4), &band1()).is_err());

    let file = SparseFile::new(bytes.len() as u64);
    let fetch = |o: u64, n: u64| Ok(bytes[o as usize..(o + n) as usize].to_vec());
    let mut r = read_with(&file, 8, fetch, || Ok(LazyRaster::from_sparse(file.clone())?.unwrap())).unwrap();
    assert!(read_with(&file, 8, fetch, || r.read_window(Window::new(0, 0, 4, 4), &band1())).is_err());
}

#[test]
fn limits_are_the_callers_to_set() {
    // A valid 4 x 4 Float64 strip: 128 bytes decoded.
    let bytes = single_strip(4, 4, 128, false);
    let read = |limits| {
        let mut r = LazyRaster::from_tiff_bytes_with_limits(bytes.clone(), limits).unwrap().unwrap();
        r.read_window(Window::full(4, 4), &band1()).map(|ds| ds.raster.data.len())
    };
    assert_eq!(read(TiffReadLimits { chunk_bytes: 128 }).unwrap(), 16);
    assert!(read(TiffReadLimits { chunk_bytes: 127 }).is_err());
    assert_eq!(read(TiffReadLimits::unlimited()).unwrap(), 16);
}

#[test]
fn stored_byte_counts_past_the_end_of_the_file_still_read() {
    // A 4 x 4 strip whose StripByteCounts claims 4 GiB: read as far as the file goes, as GDAL does.
    let bytes = tiff(4, 4, 0xFFFF_FFF0, 128, false);
    let mut r = LazyRaster::from_tiff_bytes(bytes).unwrap().unwrap();
    assert_eq!(r.read_window(Window::full(4, 4), &band1()).unwrap().raster.data.len(), 16);
}

#[test]
fn a_window_too_large_to_allocate_is_an_error() {
    // (2^32 - 1)^2 Float64 pixels: more bytes than an allocation can hold.
    let mut r = LazyRaster::from_tiff_bytes(single_strip(u32::MAX, u32::MAX, 128, false)).unwrap().unwrap();
    let e = r.read_window(Window::full(r.width(), r.height()), &band1()).unwrap_err();
    assert!(e.to_string().contains("too large"), "{e}");
}
