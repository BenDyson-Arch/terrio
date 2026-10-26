//! Strict coordinate-operation refusals carry their kind, so a caller can tell a missing grid (the
//! environment) from a CRS pair with no acceptable operation, whatever the message says.

use terrio_crs::{Error, SpatialRef, TransformOptions, Transformer};

#[test]
fn strict_refusals_have_their_own_variants() {
    // No installed or downloadable grids in this test executable.
    terrio_crs::set_grid_search_paths(Some(vec![]));
    terrio_crs::set_network_enabled(Some(false));

    // CH1903 to CH1903+: the best operation needs a grid (GDAL's ONLY_BEST=YES refuses it).
    let (src, dst) = (SpatialRef::parse("EPSG:21781").unwrap(), SpatialRef::parse("EPSG:2056").unwrap());
    let only_best = TransformOptions { only_best: true, ..Default::default() };
    let t = Transformer::with_options(&src, &dst, &only_best).unwrap();
    let e = t.transform(600000.0, 200000.0).unwrap_err();
    assert!(matches!(e, Error::MissingGrid(_)), "{e:?}");
    assert!(matches!(t.check_policy(), Err(Error::MissingGrid(_))));
    // The message is the same as before.
    assert!(e.to_string().starts_with("coordinate reference system: ONLY_BEST=YES"), "{e}");

    // Tokyo to WGS 84 has only a ballpark operation (GDAL's ALLOW_BALLPARK=NO refuses it).
    let (src, dst) = (SpatialRef::parse("EPSG:4610").unwrap(), SpatialRef::parse("EPSG:4326").unwrap());
    let no_ballpark = TransformOptions { allow_ballpark: Some(false), ..Default::default() };
    let e = Transformer::with_options(&src, &dst, &no_ballpark).err();
    assert!(matches!(e, Some(Error::NoOperation(_))), "{e:?}");

    // WGS 84 + EGM96 height to WGS 84 3D without the geoid grid: the same two kinds.
    let (src, dst) = (SpatialRef::parse("EPSG:4326+5773").unwrap(), SpatialRef::parse("EPSG:4979").unwrap());
    let t = Transformer::with_options(&src, &dst, &only_best).unwrap();
    assert!(matches!(t.transform_3d(10.0, 50.0, 100.0), Err(Error::MissingGrid(_))));
    let t = Transformer::with_options(&src, &dst, &no_ballpark).unwrap();
    assert!(matches!(t.transform_3d(10.0, 50.0, 100.0), Err(Error::NoOperation(_))));

    // A definition error stays `Crs`.
    assert!(matches!(SpatialRef::parse("+proj=nonsense"), Err(Error::Crs(_))));
}
