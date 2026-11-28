-- Run only on a disposable fixture database with PostGIS installed.
-- Independent analytical geometry oracle, not an OVG API acceptance test.
\set ON_ERROR_STOP on
BEGIN;
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE TEMP TABLE qa_parcels(zone text, geom geometry(Polygon,3857));
INSERT INTO qa_parcels VALUES
 ('A',ST_MakeEnvelope(0,0,100,100,3857)),
 ('A',ST_MakeEnvelope(100,0,200,100,3857)),
 ('B',ST_MakeEnvelope(200,0,300,100,3857));
DO $$
DECLARE n integer; a double precision; b double precision;
BEGIN
 SELECT count(*), sum(ST_Area(ST_Intersection(geom,ST_MakeEnvelope(50,0,150,100,3857))))
 INTO n,a FROM qa_parcels WHERE ST_Intersects(geom,ST_MakeEnvelope(50,0,150,100,3857));
 IF n<>2 OR abs(a-10000)>1e-6 THEN RAISE EXCEPTION 'Clip mismatch % %',n,a; END IF;
 SELECT ST_Area(ST_Union(geom)) INTO a FROM qa_parcels WHERE zone='A';
 SELECT ST_Area(ST_Union(geom)) INTO b FROM qa_parcels WHERE zone='B';
 IF abs(a-20000)>1e-6 OR abs(b-10000)>1e-6 THEN RAISE EXCEPTION 'Dissolve mismatch % %',a,b; END IF;
 SELECT ST_Area(ST_Buffer(ST_SetSRID(ST_MakePoint(50,50),3857),10,'quad_segs=8')) INTO a;
 IF abs(a-1600*sin(pi()/16))>1e-6 THEN RAISE EXCEPTION 'Buffer mismatch %',a; END IF;
 IF ST_IsValid(ST_GeomFromText('POLYGON((0 0,100 100,0 100,100 0,0 0))',3857)) THEN RAISE EXCEPTION 'Invalid fixture accepted'; END IF;
 RAISE NOTICE 'PASS clip, dissolve, buffer, invalid polygon';
END $$;
ROLLBACK;
