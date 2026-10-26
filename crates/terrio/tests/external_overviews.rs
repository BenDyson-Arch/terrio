//! External overviews: the images of a GDAL `.ovr` file next to a GeoTIFF without overviews of
//! its own are its overviews, as GDAL reads them.

use terrio::raster::RasterData;
use terrio::{DataType, RasterConfig, RasterDataset, RasterWriteOptions, Window, WindowOptions};

fn write(path: &std::path::Path, cols: usize, rows: usize, value: impl Fn(usize) -> u8, overviews: Vec<usize>) {
    let cfg = RasterConfig { cols, rows, bands: 1, data_type: DataType::U8, ..Default::default() };
    let r = terrio::Raster::from_data_native(cfg, RasterData::U8((0..cols * rows).map(value).collect())).unwrap();
    let options = RasterWriteOptions { overviews, ..Default::default() };
    terrio::write_raster_dataset(&RasterDataset::new(r, false), path.to_str().unwrap(), &options).unwrap();
}

fn overview(path: &std::path::Path, w: Window) -> Vec<f64> {
    let mut r = terrio::open_raster_lazy(path.to_str().unwrap()).unwrap();
    r.read_window(w, &WindowOptions { bands: vec![1], overview: Some(0) }).unwrap().raster.data.to_f64_vec()
}

#[test]
fn ovr_images_are_overviews() {
    let dir = tempfile::tempdir().unwrap();
    let tif = dir.path().join("img.tif");
    write(&tif, 70, 50, |i| (i % 7) as u8, Vec::new());
    assert!(terrio::open_raster_lazy(tif.to_str().unwrap()).unwrap().overviews().is_empty());
    // An .ovr of one 35 x 25 image, with values of its own.
    write(&dir.path().join("ovr.tif"), 35, 25, |i| (100 + i % 11) as u8, Vec::new());
    std::fs::rename(dir.path().join("ovr.tif"), dir.path().join("img.tif.ovr")).unwrap();
    let r = terrio::open_raster_lazy(tif.to_str().unwrap()).unwrap();
    assert_eq!(r.overviews(), [(35, 25)]);
    assert_eq!(r.level_geo_transform(Some(0)).unwrap()[1], 2.0);
    let got = overview(&tif, Window::new(3, 4, 5, 2));
    let want: Vec<f64> = [4, 5].iter().flat_map(|y| (3..8).map(move |x| (100 + (y * 35 + x) % 11) as f64)).collect();
    assert_eq!(got, want);
    // Full resolution is the TIFF's.
    let mut r = r;
    let full = r.read_window(Window::new(0, 0, 3, 1), &WindowOptions { bands: vec![1], overview: None }).unwrap();
    assert_eq!(full.raster.data.to_f64_vec(), [0.0, 1.0, 2.0]);
}

#[test]
fn internal_overviews_win_over_an_ovr() {
    let dir = tempfile::tempdir().unwrap();
    let tif = dir.path().join("img.tif");
    write(&tif, 70, 50, |i| (i % 7) as u8, vec![2]);
    write(&dir.path().join("ovr.tif"), 35, 25, |_| 200, Vec::new());
    std::fs::rename(dir.path().join("ovr.tif"), dir.path().join("img.tif.ovr")).unwrap();
    assert!(overview(&tif, Window::new(0, 0, 4, 4)).iter().all(|v| *v < 7.0));
}

#[test]
fn an_ovr_of_another_type_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let tif = dir.path().join("img.tif");
    write(&tif, 70, 50, |i| (i % 7) as u8, Vec::new());
    let cfg = RasterConfig { cols: 35, rows: 25, bands: 1, data_type: DataType::F32, ..Default::default() };
    let r = terrio::Raster::from_data_native(cfg, RasterData::F32(vec![1.0; 35 * 25])).unwrap();
    let ovr = dir.path().join("ovr.tif");
    terrio::write_raster_dataset(&RasterDataset::new(r, false), ovr.to_str().unwrap(), &RasterWriteOptions::default()).unwrap();
    std::fs::rename(&ovr, dir.path().join("img.tif.ovr")).unwrap();
    assert!(terrio::open_raster_lazy(tif.to_str().unwrap()).unwrap().overviews().is_empty());
}
