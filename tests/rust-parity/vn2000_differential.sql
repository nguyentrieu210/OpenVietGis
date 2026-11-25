-- Read-only VN-2000 differential probe. Output (points) is recorded in
-- crates/ovg-spatial/tests/fixtures/vn2000_postgis_differential.json.
\pset tuples_only on
\pset format unaligned
WITH pts(id, epsg, lon, lat) AS (VALUES
 ('qn-hvs-anchorage',5899,107.165,20.715),('qn-halong',5899,107.08,21.03),('danang',5899,108.2,16.05),('dongnai',5899,107.05,10.95),('lamdong',5899,108.44,11.94),
 ('tm3-102-north',5896,102.5,22.0),('tm3-102-edge',5896,103.4,21.2),
 ('tm3-105-hanoi',5897,105.85,21.03),('tm3-105-camau',5897,105.15,9.18),
 ('tm3-108-hue',5898,107.59,16.46),('tm3-108-daklak',5898,108.05,12.68),
 ('dienbien',9205,103.02,21.39),('laichau',9205,103.46,22.40),
 ('laocai',9206,103.97,22.48),('sonla',9206,103.9,21.33),
 ('yenbai',9207,104.87,21.72),('kiengiang',9207,104.98,10.0),('phuquoc',9207,103.96,10.22),
 ('hagiang',9208,104.98,22.82),
 ('hanoi-9209',9209,105.85,21.03),('hoabinh',9209,105.34,20.81),('dongthap',9209,105.63,10.46),
 ('hcmc',9210,106.7,10.78),('bacninh',9210,106.07,21.18),
 ('hungyen',9211,106.05,20.65),
 ('namdinh',9212,106.17,20.42),
 ('haiphong',9213,106.68,20.86),('quangbinh',9213,106.6,17.47),
 ('quangtri',9214,107.1,16.75),
 ('langson',9215,106.76,21.85),
 ('binhphuoc',9216,106.9,11.75),
 ('binhdinh',9217,109.1,13.78),('ninhthuan',9217,108.99,11.56),
 ('phuyen',9218,109.3,13.09),('khanhhoa',9218,109.19,12.24),
 ('utm48-north',3405,103.5,21.5),('utm48-south',3405,107.5,10.5),
 ('utm49-central',3406,108.5,14.0),('utm49-south',3406,109.3,12.0)
), g AS (
 SELECT *, ST_SetSRID(ST_MakePoint(lon,lat),4756) AS geom FROM pts
), f AS (
 SELECT *, ST_Transform(ST_SetSRID(geom,0), 'EPSG:4756', 'EPSG:'||epsg) AS proj,
   ST_Transform(geom, 4326) AS wgs_default,
   ST_TransformPipeline(geom, 'urn:ogc:def:coordinateOperation:EPSG::6960', 4326) AS wgs_6960
 FROM g
), r AS (
 SELECT *, ST_SetSRID(ST_MakePoint(round(ST_X(proj)::numeric,3)::float8, round(ST_Y(proj)::numeric,3)::float8),0) AS proj_in,
   ST_SetSRID(ST_MakePoint(round(ST_X(wgs_6960)::numeric,9)::float8, round(ST_Y(wgs_6960)::numeric,9)::float8),4326) AS wgs_in
 FROM f
)
SELECT json_build_object(
 'postgis', postgis_full_version(),
 'points', json_agg(json_build_object(
  'id', id, 'epsg', epsg,
  'vn2000_lonlat', json_build_array(lon, lat),
  'projected', json_build_array(ST_X(proj), ST_Y(proj)),
  'wgs84_st_transform', json_build_array(ST_X(wgs_default), ST_Y(wgs_default)),
  'wgs84_pipeline_6960', json_build_array(ST_X(wgs_6960), ST_Y(wgs_6960)),
  'projected_input', json_build_array(ST_X(proj_in), ST_Y(proj_in)),
  'projected_input_to_vn2000', (SELECT json_build_array(ST_X(t), ST_Y(t)) FROM ST_Transform(proj_in, 'EPSG:'||epsg, 'EPSG:4756') t),
  'projected_input_to_wgs84', (SELECT json_build_array(ST_X(t), ST_Y(t)) FROM ST_TransformPipeline(ST_SetSRID(ST_Transform(proj_in, 'EPSG:'||epsg, 'EPSG:4756'),4756), 'urn:ogc:def:coordinateOperation:EPSG::6960', 4326) t),
  'wgs84_input', json_build_array(ST_X(wgs_in), ST_Y(wgs_in)),
  'wgs84_input_to_vn2000_st_transform', (SELECT json_build_array(ST_X(t), ST_Y(t)) FROM ST_Transform(wgs_in, 4756) t),
  'wgs84_input_to_vn2000_pipeline_6960', (SELECT json_build_array(ST_X(t), ST_Y(t)) FROM ST_InverseTransformPipeline(wgs_in, 'urn:ogc:def:coordinateOperation:EPSG::6960', 4756) t),
  'wgs84_input_to_projected', (SELECT json_build_array(ST_X(t), ST_Y(t)) FROM ST_Transform(ST_SetSRID(ST_InverseTransformPipeline(wgs_in, 'urn:ogc:def:coordinateOperation:EPSG::6960', 4756),0), 'EPSG:4756', 'EPSG:'||epsg) t)
 ) ORDER BY epsg, id))
FROM r;
