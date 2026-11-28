CREATE TABLE gpkg_spatial_ref_sys(srs_name TEXT NOT NULL,srs_id INTEGER NOT NULL PRIMARY KEY,organization TEXT NOT NULL,organization_coordsys_id INTEGER NOT NULL,definition TEXT NOT NULL,description TEXT);
INSERT INTO gpkg_spatial_ref_sys VALUES('Undefined Cartesian',-1,'NONE',-1,'undefined','undefined Cartesian coordinate reference system'),('Undefined geographic',0,'NONE',0,'undefined','undefined geographic coordinate reference system'),('WGS 84',4326,'EPSG',4326,'GEOGCS["WGS 84",DATUM["WGS_1984",SPHEROID["WGS 84",6378137,298.257223563]],PRIMEM["Greenwich",0],UNIT["degree",0.0174532925199433],AUTHORITY["EPSG","4326"]]','longitude latitude XY');
CREATE TABLE gpkg_contents(table_name TEXT NOT NULL PRIMARY KEY,data_type TEXT NOT NULL,identifier TEXT UNIQUE,description TEXT DEFAULT '',last_change DATETIME NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),min_x DOUBLE,min_y DOUBLE,max_x DOUBLE,max_y DOUBLE,srs_id INTEGER REFERENCES gpkg_spatial_ref_sys(srs_id));
CREATE TABLE gpkg_geometry_columns(table_name TEXT NOT NULL,column_name TEXT NOT NULL,geometry_type_name TEXT NOT NULL,srs_id INTEGER NOT NULL,z TINYINT NOT NULL,m TINYINT NOT NULL,PRIMARY KEY(table_name,column_name),FOREIGN KEY(table_name) REFERENCES gpkg_contents(table_name),FOREIGN KEY(srs_id) REFERENCES gpkg_spatial_ref_sys(srs_id));
CREATE TABLE gpkg_extensions(table_name TEXT,column_name TEXT,extension_name TEXT NOT NULL,definition TEXT NOT NULL,scope TEXT NOT NULL,UNIQUE(table_name,column_name,extension_name));
CREATE TABLE features(fid INTEGER PRIMARY KEY AUTOINCREMENT,stable_id TEXT NOT NULL UNIQUE,revision INTEGER NOT NULL CHECK(revision>0),geom GEOMETRY NOT NULL,attributes TEXT NOT NULL CHECK(json_valid(attributes)));
INSERT INTO gpkg_contents(table_name,data_type,identifier,srs_id) VALUES('features','features','OpenVietGIS local features',4326);
INSERT INTO gpkg_geometry_columns VALUES('features','geom','GEOMETRY',4326,0,0);
CREATE VIRTUAL TABLE rtree_features_geom USING rtree(id,minx,maxx,miny,maxy);
INSERT INTO gpkg_extensions VALUES('features','geom','gpkg_rtree_index','http://www.geopackage.org/spec/#extension_rtree','write-only');
CREATE TRIGGER rtree_features_geom_insert AFTER INSERT ON features BEGIN
  INSERT OR REPLACE INTO rtree_features_geom VALUES(new.fid,ST_MinX(new.geom),ST_MaxX(new.geom),ST_MinY(new.geom),ST_MaxY(new.geom));
END;
CREATE TRIGGER rtree_features_geom_update AFTER UPDATE OF geom,fid ON features BEGIN
  DELETE FROM rtree_features_geom WHERE id=old.fid;
  INSERT OR REPLACE INTO rtree_features_geom VALUES(new.fid,ST_MinX(new.geom),ST_MaxX(new.geom),ST_MinY(new.geom),ST_MaxY(new.geom));
END;
CREATE TRIGGER rtree_features_geom_delete AFTER DELETE ON features BEGIN
  DELETE FROM rtree_features_geom WHERE id=old.fid;
END;
CREATE TABLE ovg_local_metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL);
INSERT INTO ovg_local_metadata VALUES(1,1);
CREATE TABLE ovg_feature_revision(stable_id TEXT PRIMARY KEY,revision INTEGER NOT NULL,deleted INTEGER NOT NULL CHECK(deleted IN(0,1)));
CREATE TABLE ovg_journal(operation_id TEXT PRIMARY KEY,stable_id TEXT NOT NULL,payload_hash BLOB NOT NULL CHECK(length(payload_hash)=32),revision INTEGER NOT NULL,payload BLOB NOT NULL);
