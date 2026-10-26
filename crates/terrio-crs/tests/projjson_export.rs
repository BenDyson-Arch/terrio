//! `SpatialRef::to_projjson`: one CRS of each kind against what PROJ 9.9 writes for it, read back
//! by `SpatialRef::from_projjson` where its subset allows, and the refusals. The full comparison
//! with live PROJ and GDAL (45 CRSs by meaning and coordinates, and every IAU_2015 CRS) is the
//! `projjson-export` suite of `scripts/conformance.py`.

use serde_json::{Value, json};
use terrio_crs::{SpatialRef, Transformer};

fn export(definition: &str) -> Value {
    serde_json::from_str(&SpatialRef::parse(definition).unwrap().to_projjson().unwrap()).unwrap()
}

/// The directions of a coordinate system's axes.
fn directions(cs: &Value) -> Vec<&str> {
    cs["axis"].as_array().unwrap().iter().map(|a| a["direction"].as_str().unwrap()).collect()
}

/// A conversion's or transformation's method code and `(parameter code, value)`s, the values
/// rounded to 1e-9 (degrees go through radians).
fn method(c: &Value) -> (u64, Vec<(u64, f64)>) {
    let value = |p: &Value| (p["value"].as_f64().unwrap() * 1e9).round() / 1e9;
    let params = c["parameters"].as_array().unwrap().iter().map(|p| (p["id"]["code"].as_u64().unwrap(), value(p)));
    (c["method"]["id"]["code"].as_u64().unwrap(), params.collect())
}

#[test]
fn geographic_and_geocentric_crss() {
    let v = export("EPSG:4326");
    assert_eq!(v["$schema"], "https://proj.org/schemas/v0.7/projjson.schema.json");
    assert_eq!((&v["type"], &v["id"]), (&json!("GeographicCRS"), &json!({"authority": "EPSG", "code": 4326})));
    let e = &v["datum"]["ellipsoid"];
    assert_eq!((e["semi_major_axis"].as_f64(), e["inverse_flattening"].as_f64()), (Some(6378137.0), Some(298.257223563)));
    assert_eq!(directions(&v["coordinate_system"]), ["north", "east"]);
    assert_eq!(directions(&export("EPSG:4979")["coordinate_system"]), ["north", "east", "up"]);
    let v = export("EPSG:4978");
    assert_eq!((&v["type"], &v["coordinate_system"]["subtype"]), (&json!("GeodeticCRS"), &json!("Cartesian")));
    assert_eq!(directions(&v["coordinate_system"]), ["geocentricX", "geocentricY", "geocentricZ"]);
}

#[test]
fn projected_crss() {
    let v = export("EPSG:32632");
    assert_eq!(v["conversion"]["name"], "UTM zone 32N");
    assert_eq!(method(&v["conversion"]), (9807, vec![(8801, 0.0), (8802, 9.0), (8805, 0.9996), (8806, 500000.0), (8807, 0.0)]));
    let v = export("EPSG:2154");
    let (code, mut params) = method(&v["conversion"]);
    params.sort_by_key(|p| p.0);
    assert_eq!((code, params), (9802, vec![(8821, 46.5), (8822, 3.0), (8823, 49.0), (8824, 44.0), (8826, 700000.0), (8827, 6600000.0)]));
    assert_eq!(directions(&v["coordinate_system"]), ["east", "north"]);
}

#[test]
fn vertical_compound_and_bound_crss() {
    let v = export("EPSG:5773");
    assert_eq!((&v["type"], &v["id"]["code"]), (&json!("VerticalCRS"), &json!(5773)));
    assert_eq!(directions(&v["coordinate_system"]), ["up"]);
    let v = export("EPSG:9707");
    let parts: Vec<&str> = v["components"].as_array().unwrap().iter().map(|c| c["type"].as_str().unwrap()).collect();
    assert_eq!((&v["type"], parts), (&json!("CompoundCRS"), vec!["GeographicCRS", "VerticalCRS"]));
    let v = export("+proj=longlat +ellps=bessel +towgs84=598.1,73.7,418.2,0.202,0.045,-2.455,6.7");
    assert_eq!((&v["type"], &v["target_crs"]["id"]["code"]), (&json!("BoundCRS"), &json!(4326)));
    let towgs84 = [(8605, 598.1), (8606, 73.7), (8607, 418.2), (8608, 0.202), (8609, 0.045), (8610, -2.455), (8611, 6.7)];
    assert_eq!(method(&v["transformation"]), (9606, towgs84.to_vec()));
}

#[test]
fn planetographic_and_planetocentric_crss() {
    // West-positive longitudes are a west axis; planetocentric latitudes a geodetic CRS with a
    // spherical coordinate system; IAU CRSs keep their versioned identifier.
    let v = export("IAU_2015:49901");
    assert_eq!((&v["type"], &v["id"]), (&json!("GeographicCRS"), &json!({"authority": "IAU", "code": 49901, "version": 2015})));
    assert_eq!(directions(&v["coordinate_system"]), ["north", "west"]);
    let v = export("IAU_2015:49902");
    assert_eq!((&v["type"], &v["coordinate_system"]["subtype"]), (&json!("GeodeticCRS"), &json!("spherical")));
    assert_eq!(v["coordinate_system"]["axis"][0]["name"], "Planetocentric latitude");
    assert_eq!(directions(&export("+proj=longlat +a=2440530 +b=2438260 +axis=nwu")["coordinate_system"]), ["north", "west"]);
    assert_eq!(export("+proj=longlat +a=2440530 +b=2438260 +geoc")["coordinate_system"]["subtype"], "spherical");
    // A semi-minor axis given as such is written as such: Eros's rounded inverse flattening moved
    // an orthographic point 3e-6° from GDAL's.
    let e = &export("IAU_2015:200043371")["base_crs"]["datum"]["ellipsoid"];
    assert_eq!((e["semi_major_axis"].as_f64(), e["semi_minor_axis"].as_f64()), (Some(17000.0), Some(5500.0)));
}

#[test]
fn what_would_be_written_wrongly_is_refused() {
    let refusal = |d: &str| SpatialRef::parse(d).unwrap().to_projjson().unwrap_err().to_string();
    assert!(SpatialRef::from_epsg(9000).unwrap().to_projjson().unwrap_err().to_string().contains("dynamic"));
    assert!(refusal("+proj=robin +datum=WGS84").contains("+proj=robin"));
    // A projection on the conformal or authalic sphere of an ellipsoid (IAU_2015:19991).
    assert!(refusal("+proj=merc +R_C +lat_0=0 +lon_0=0 +a=2440530 +b=2438260").contains("auxiliary sphere"));
    assert!(refusal("+proj=laea +R_A +lat_0=52 +lon_0=10 +ellps=GRS80").contains("auxiliary sphere"));
    // On a sphere, the auxiliary sphere is the sphere: written.
    assert!(SpatialRef::parse("+proj=merc +R_C +R=696000000").unwrap().to_projjson().is_ok());
}

#[test]
fn projjson_reads_back_as_the_same_crs() {
    terrio_crs::set_grid_search_paths(Some(Vec::new()));
    terrio_crs::set_network_enabled(Some(false));
    let wgs84 = SpatialRef::from_epsg(4326).unwrap();
    for (definition, (x, y)) in [
        ("EPSG:4326", (10.0, 50.0)),
        ("EPSG:32632", (500000.0, 5500000.0)),
        ("EPSG:2154", (650000.0, 6860000.0)),
        ("+proj=longlat +ellps=bessel +towgs84=598.1,73.7,418.2,0.202,0.045,-2.455,6.7", (8.0, 47.0)),
        // West-positive projected axes, as IAU_2015 planetographic projected CRSs have them.
        ("+proj=tmerc +lat_0=0 +lon_0=10 +k=1 +ellps=WGS84 +axis=wnu", (-100000.0, 5000000.0)),
    ] {
        let original = SpatialRef::parse(definition).unwrap();
        let back = SpatialRef::from_projjson(&original.to_projjson().unwrap()).unwrap();
        let a = Transformer::new(&original, &wgs84).unwrap().transform(x, y).unwrap();
        let b = Transformer::new(&back, &wgs84).unwrap().transform(x, y).unwrap();
        assert!((a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9, "{definition}: {a:?} {b:?}");
    }
}

/// For `scripts/conformance.py` (`projjson-export`): writes the PROJJSON of each definition of
/// `TERRIO_PROJJSON_EXPORT_CASES` (one per line) to `TERRIO_PROJJSON_EXPORT_REPORT`, as JSON
/// lines of `{definition, projjson}` or `{definition, error}`. Does nothing without them.
#[test]
fn export_report() {
    let (Ok(cases), Ok(report)) = (std::env::var("TERRIO_PROJJSON_EXPORT_CASES"), std::env::var("TERRIO_PROJJSON_EXPORT_REPORT")) else {
        return;
    };
    let mut out = String::new();
    for definition in std::fs::read_to_string(cases).unwrap().lines().filter(|l| !l.trim().is_empty()) {
        let line = match SpatialRef::parse(definition).and_then(|s| s.to_projjson()) {
            Ok(text) => json!({"definition": definition, "projjson": serde_json::from_str::<Value>(&text).unwrap()}),
            Err(e) => json!({"definition": definition, "error": e.to_string()}),
        };
        out.push_str(&line.to_string());
        out.push('\n');
    }
    std::fs::write(report, out).unwrap();
}
