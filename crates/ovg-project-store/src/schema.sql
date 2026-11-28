CREATE TABLE project(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL CHECK(revision>=0));
INSERT INTO project VALUES(1,0);
CREATE TABLE layer(id TEXT PRIMARY KEY,name TEXT NOT NULL,schema_json TEXT);
CREATE TABLE feature_revision(layer TEXT NOT NULL REFERENCES layer(id),id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),PRIMARY KEY(layer,id));
CREATE TABLE feature(fid INTEGER PRIMARY KEY AUTOINCREMENT,layer TEXT NOT NULL,id TEXT NOT NULL,wkb BLOB NOT NULL,attributes TEXT NOT NULL CHECK(json_valid(attributes)),minx REAL NOT NULL,maxx REAL NOT NULL,miny REAL NOT NULL,maxy REAL NOT NULL,UNIQUE(layer,id),FOREIGN KEY(layer,id) REFERENCES feature_revision(layer,id));
CREATE VIRTUAL TABLE feature_index USING rtree(fid,minx,maxx,miny,maxy);
CREATE TRIGGER feature_insert AFTER INSERT ON feature BEGIN INSERT INTO feature_index VALUES(NEW.fid,NEW.minx,NEW.maxx,NEW.miny,NEW.maxy); END;
CREATE TRIGGER feature_update AFTER UPDATE OF minx,maxx,miny,maxy ON feature BEGIN UPDATE feature_index SET minx=NEW.minx,maxx=NEW.maxx,miny=NEW.miny,maxy=NEW.maxy WHERE fid=NEW.fid; END;
CREATE TRIGGER feature_delete AFTER DELETE ON feature BEGIN DELETE FROM feature_index WHERE fid=OLD.fid; END;
CREATE TABLE relation(id TEXT PRIMARY KEY,source_layer TEXT NOT NULL REFERENCES layer(id),target_layer TEXT NOT NULL REFERENCES layer(id),kind TEXT NOT NULL CHECK(kind IN('one_one','one_many','many_many')));
CREATE TABLE edge(relation TEXT NOT NULL REFERENCES relation(id),source_layer TEXT NOT NULL,source_id TEXT NOT NULL,target_layer TEXT NOT NULL,target_id TEXT NOT NULL,PRIMARY KEY(relation,source_id,target_id),FOREIGN KEY(source_layer,source_id) REFERENCES feature(layer,id) DEFERRABLE INITIALLY DEFERRED,FOREIGN KEY(target_layer,target_id) REFERENCES feature(layer,id) DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX edge_reverse ON edge(relation,target_id,source_id);
CREATE TABLE receipt(operation TEXT PRIMARY KEY,digest BLOB NOT NULL CHECK(length(digest)=32),revision INTEGER NOT NULL,reply TEXT NOT NULL);
CREATE TABLE journal(revision INTEGER PRIMARY KEY,operation TEXT NOT NULL UNIQUE REFERENCES receipt(operation),before_json TEXT NOT NULL,after_json TEXT NOT NULL);
CREATE INDEX feature_layer_fid ON feature(layer,fid);
CREATE TABLE legacy_source(layer TEXT PRIMARY KEY REFERENCES layer(id),sha256 TEXT NOT NULL CHECK(length(sha256)=64),source_bytes INTEGER NOT NULL,live_count INTEGER NOT NULL,tombstone_count INTEGER NOT NULL);
CREATE TABLE legacy_receipt(layer TEXT NOT NULL REFERENCES layer(id),operation TEXT NOT NULL,feature TEXT NOT NULL,digest BLOB NOT NULL CHECK(length(digest)=32),revision INTEGER NOT NULL,payload BLOB NOT NULL,PRIMARY KEY(layer,operation));
CREATE TABLE legacy_history(layer TEXT NOT NULL REFERENCES layer(id),seq INTEGER NOT NULL,command TEXT NOT NULL,before_json TEXT NOT NULL,after_json TEXT NOT NULL,applied INTEGER NOT NULL CHECK(applied IN(0,1)),PRIMARY KEY(layer,seq));
CREATE TABLE legacy_command(layer TEXT NOT NULL REFERENCES layer(id),command TEXT NOT NULL,PRIMARY KEY(layer,command));
-- V3 additive assets and bulk import
CREATE TABLE project_asset(id TEXT PRIMARY KEY,kind TEXT NOT NULL,epsg INTEGER,bytes INTEGER NOT NULL CHECK(bytes>0 AND bytes<=536870912),sha256 TEXT NOT NULL CHECK(length(sha256)=64));
CREATE TABLE project_asset_chunk(asset TEXT NOT NULL REFERENCES project_asset(id) ON DELETE CASCADE,seq INTEGER NOT NULL CHECK(seq>=0),data BLOB NOT NULL CHECK(length(data)>0 AND length(data)<=1048576),PRIMARY KEY(asset,seq));
CREATE TABLE bulk_import(operation TEXT PRIMARY KEY,digest TEXT NOT NULL CHECK(length(digest)=64),layer TEXT NOT NULL REFERENCES layer(id),revision INTEGER NOT NULL,count INTEGER NOT NULL);
-- V4 additive survey records
CREATE TABLE survey_track(layer TEXT NOT NULL REFERENCES layer(id),id TEXT NOT NULL,name TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN('recording','paused','finished')),segment INTEGER NOT NULL CHECK(segment>=0),PRIMARY KEY(layer,id));
CREATE TABLE survey_fix(layer TEXT NOT NULL,track_id TEXT NOT NULL,seq INTEGER NOT NULL CHECK(seq>0),segment INTEGER NOT NULL CHECK(segment>0),utc_ms INTEGER NOT NULL CHECK(utc_ms>0),fix_json TEXT NOT NULL CHECK(json_valid(fix_json)),PRIMARY KEY(layer,track_id,seq),FOREIGN KEY(layer,track_id) REFERENCES survey_track(layer,id) ON DELETE CASCADE);
-- V5 additive encrypted media
CREATE TABLE media_domain(singleton INTEGER PRIMARY KEY CHECK(singleton=1),scope TEXT NOT NULL UNIQUE);
CREATE TABLE encrypted_media(id TEXT PRIMARY KEY,scope TEXT NOT NULL REFERENCES media_domain(scope),layer TEXT NOT NULL,feature TEXT NOT NULL,source_scope TEXT NOT NULL,source_id TEXT NOT NULL,source_sha BLOB NOT NULL CHECK(length(source_sha)=32),cipher_sha BLOB NOT NULL CHECK(length(cipher_sha)=32),cipher_bytes INTEGER NOT NULL CHECK(cipher_bytes>48 AND cipher_bytes<=16777264),ciphertext BLOB NOT NULL CHECK(length(ciphertext)=cipher_bytes),UNIQUE(source_scope,source_id),FOREIGN KEY(layer,feature) REFERENCES feature_revision(layer,id));
CREATE INDEX encrypted_media_owner ON encrypted_media(layer,feature,id);
-- V6 additive native CAD versions
CREATE TABLE cad_version(asset TEXT PRIMARY KEY REFERENCES project_asset(id) ON DELETE CASCADE,parent TEXT NOT NULL REFERENCES project_asset(id) ON DELETE RESTRICT,parent_sha TEXT NOT NULL CHECK(length(parent_sha)=64),CHECK(asset<>parent));
CREATE INDEX cad_version_parent ON cad_version(parent);
-- V7 additive native media capture
CREATE TABLE media_capture(id TEXT PRIMARY KEY REFERENCES encrypted_media(id) ON DELETE CASCADE,mime TEXT NOT NULL CHECK(mime='image/jpeg'));
