#!/usr/bin/env python3
"""Compare terrio's PROJJSON export (`SpatialRef::to_projjson`) with live PROJ and GDAL.

45 CRSs are compared with PROJ's PROJJSON by meaning (types, identifiers, datums, ellipsoids,
prime meridians, axes, methods and parameters by EPSG code in SI units) and read by GDAL, whose
transformation of a point must match the original CRS's within 1e-8 degrees. Every IAU_2015 CRS is
exported and read by GDAL the same way, reported per body; the only refusals allowed there are
projections without an EPSG method and projections on an auxiliary sphere of an ellipsoid.
"""
import argparse
import json
import math
import os
import sqlite3
import subprocess
import tempfile
from collections import defaultdict
from pathlib import Path

from projjson_reference_env import no_grid_proj

from osgeo import osr

ROOT = Path(__file__).resolve().parents[1]
DEG = math.pi / 180
DEFINITIONS = [
    # Geographic 2D, 3D and geocentric.
    'EPSG:4326', 'EPSG:4258', 'EPSG:4269', 'EPSG:4267', 'EPSG:4277', 'EPSG:4807', 'EPSG:4230', 'EPSG:4283', 'EPSG:4148',
    'EPSG:4979', 'EPSG:4937', 'EPSG:4978', 'EPSG:4936',
    # Projected: one or more CRSs per method, feet, grads, axis orders.
    'EPSG:32632', 'EPSG:32761', 'EPSG:27700', 'EPSG:2263', 'EPSG:3857', 'EPSG:2154', 'EPSG:28992', 'EPSG:3035', 'EPSG:31467',
    'EPSG:27572', 'EPSG:2056', 'EPSG:21781', 'EPSG:3031', 'EPSG:5070', 'EPSG:3395', 'EPSG:2193', 'EPSG:6933', 'EPSG:3078',
    'EPSG:29873', 'EPSG:5514', 'EPSG:2065', 'EPSG:4087', 'EPSG:3034', 'EPSG:3112',
    # Vertical and compound.
    'EPSG:5773', 'EPSG:5703', 'EPSG:3855', 'EPSG:9707', 'EPSG:7415', 'EPSG:4326+5773',
    # Bound.
    '+proj=utm +zone=33 +ellps=intl +towgs84=-87,-98,-121',
    '+proj=longlat +ellps=bessel +towgs84=598.1,73.7,418.2,0.202,0.045,-2.455,6.7',
]
ALLOWED_REFUSALS = ('no PROJJSON for the projection', 'no PROJJSON for a projection on an auxiliary sphere')


def srs(text):
    s = osr.SpatialReference()
    s.SetFromUserInput(text)
    s.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    return s


def factor(unit):
    if isinstance(unit, dict):
        return unit['conversion_factor']
    return {'degree': DEG, 'grad': math.pi / 200, 'arc-second': DEG / 3600, 'parts per million': 1e-6}.get(unit, 1.0)


def si(v, default):
    return v['value'] * factor(v.get('unit')) if isinstance(v, dict) else v * default


def canonical(v):
    """What the comparison looks at (as PROJ and terrio may name and identify inner objects)."""
    out = {'type': v.get('type'), 'name': None if 'id' in v else v.get('name'), 'id': v.get('id', {}).get('code')}
    d = v.get('datum') or v.get('datum_ensemble')
    if d and 'ellipsoid' in d:  # not vertical datums
        e = d['ellipsoid']
        a = si(e['semi_major_axis'] if 'semi_major_axis' in e else e['radius'], 1)
        rf = e.get('inverse_flattening') or (a / (a - si(e['semi_minor_axis'], 1)) if 'semi_minor_axis' in e else 0.0)
        pm = si(d['prime_meridian']['longitude'], DEG) if 'prime_meridian' in d else 0.0
        out['datum'] = [round(a, 3), round(rf, 6), round(pm, 11)]
    if 'coordinate_system' in v:
        axes = [[a['direction'], round(factor(a.get('unit')), 15)] for a in v['coordinate_system']['axis']]
        if any('meridian' in a for a in v['coordinate_system']['axis']):  # polar CSs: east and north in terrio
            axes[0][0], axes[1][0] = 'east', 'north'
        out['cs'] = [v['coordinate_system']['subtype'], axes]
    for key in ('conversion', 'transformation'):
        if key in v:
            c = v[key]
            params = sorted((p['id']['code'], round(si({'value': p['value'], 'unit': p.get('unit')}, 1), 9)) for p in c['parameters'])
            out[key] = [c['method'].get('id', {}).get('code'), params]
    for key in ('base_crs', 'source_crs', 'target_crs'):
        if key in v:
            out[key] = {k: x for k, x in canonical(v[key]).items() if k not in ('name', 'cs')}
    if 'components' in v:
        out['components'] = [dict(canonical(c), **({'id': None, 'name': None} if 'id' in v else {})) for c in v['components']]
    return out


def export(definitions):
    """terrio's PROJJSON (or error) of each definition, through the `export_report` test."""
    with tempfile.TemporaryDirectory() as tmp:
        cases, report = Path(tmp) / 'cases.txt', Path(tmp) / 'report.jsonl'
        cases.write_text('\n'.join(definitions) + '\n')
        env = dict(os.environ, TERRIO_PROJJSON_EXPORT_CASES=str(cases), TERRIO_PROJJSON_EXPORT_REPORT=str(report))
        subprocess.run(['cargo', 'test', '-q', '--release', '-p', 'terrio-crs', '--test', 'projjson_export', 'export_report'],
                       cwd=ROOT, env=env, check=True, capture_output=True)
        return {r['definition']: r for r in map(json.loads, report.read_text().splitlines())}


def difference(ours, original, base, points):
    """The largest difference, in degrees of `base`, between transforming the first point (of
    `points`, in `base`) that is in the CRSs' domain from `ours` and from `original`."""
    for p in points:
        try:
            q = osr.CoordinateTransformation(base, original).TransformPoint(*p, 0.0)
            a = osr.CoordinateTransformation(original, base).TransformPoint(*q)
            b = osr.CoordinateTransformation(ours, base).TransformPoint(*q)
        except RuntimeError:
            continue
        return max(abs(a[0] - b[0]), abs(a[1] - b[1]))
    return None


def status(d):
    return 'identical' if d == 0 else 'within tolerance' if d <= 1e-8 else 'differs'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    osr.UseExceptions()
    database = next(Path(p) / 'proj.db' for p in osr.GetPROJSearchPaths() if (Path(p) / 'proj.db').is_file())
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as db:
        iau = [f'IAU_2015:{c}' for (c,) in db.execute("SELECT code FROM crs_view WHERE auth_name = 'IAU_2015' ORDER BY CAST(code AS INTEGER)")]
    ours = export(DEFINITIONS + iau)
    results = []
    wgs84 = srs('EPSG:4326')
    for definition in DEFINITIONS:
        r = ours[definition]
        try:
            proj = json.loads(srs(definition + (' +type=crs' if definition.startswith('+') else '')).ExportToPROJJSON())
            if 'error' in r:
                raise ValueError(r['error'])
            if canonical(r['projjson']) != canonical(proj):
                raise ValueError(f"PROJ {canonical(proj)}\nterrio {canonical(r['projjson'])}")
            original, back = srs(definition), srs(json.dumps(r['projjson']))
            area = original.GetAreaOfUse()
            point = ((area.west_lon_degree + area.east_lon_degree) / 2, (area.south_lat_degree + area.north_lat_degree) / 2) if area else (10.0, 50.0)
            d = difference(back, original, wgs84, [point, (10.0, 50.0)]) if original.IsProjected() or original.IsGeographic() else 0.0
            results.append(dict(group='45 CRSs (PROJ by meaning, GDAL by coordinates)', name=definition, status=status(d), detail=f'{d:.3g} degrees' if d else ''))
        except Exception as error:
            results.append(dict(group='45 CRSs (PROJ by meaning, GDAL by coordinates)', name=definition, status='differs', detail=str(error)))
    bodies = defaultdict(lambda: dict(written=0, refused=0, worst=0.0, bad=[]))
    points = [(10, 20), (0, 85), (0, -85), (90, 60), (-90, -60), (180, 0), (45, 45)]
    for code in iau:
        body = bodies[int(code.split(':')[1]) // 100]
        r = ours[code]
        if 'error' in r:
            body['refused'] += 1
            if not r['error'].startswith(ALLOWED_REFUSALS, len('coordinate reference system: ')):
                body['bad'].append(f"{code}: {r['error']}")
            continue
        body['written'] += 1
        base = srs(f"IAU_2015:{int(code.split(':')[1]) // 100 * 100}")
        d = difference(srs(json.dumps(r['projjson'])), srs(code), base, points)
        if d is None or d > 1e-8:
            body['bad'].append(f'{code}: {d}')
        body['worst'] = max(body['worst'], d or 0.0)
    for n, b in sorted(bodies.items()):
        name = f'IAU_2015:{n * 100}..: {b["written"]} written, {b["refused"]} refused'
        results.append(dict(group='IAU_2015 (GDAL by coordinates)', name=name, status='differs' if b['bad'] else status(b['worst']),
                            detail='; '.join(b['bad'][:3]) or (f'{b["worst"]:.3g} degrees' if b['worst'] else '')))
    print(json.dumps(results, indent=2) if args.json else '\n'.join(f'{r["name"]}: {r["status"]} {r["detail"]}' for r in results))
    return int(any(r['status'] == 'differs' for r in results))


if __name__ == '__main__':
    with no_grid_proj():
        raise SystemExit(main())
