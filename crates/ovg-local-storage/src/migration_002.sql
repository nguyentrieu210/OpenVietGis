CREATE TABLE ovg_edit_history(seq INTEGER PRIMARY KEY AUTOINCREMENT,command_id TEXT UNIQUE NOT NULL,before_json TEXT NOT NULL,after_json TEXT NOT NULL,applied INTEGER NOT NULL CHECK(applied IN(0,1)));
CREATE TABLE ovg_attribute_schema(singleton INTEGER PRIMARY KEY CHECK(singleton=1),fields_json TEXT NOT NULL);
CREATE TABLE ovg_survey_track(id TEXT PRIMARY KEY,name TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN('recording','paused','finished')),segment INTEGER NOT NULL DEFAULT 0);
CREATE TABLE ovg_survey_fix(track_id TEXT NOT NULL REFERENCES ovg_survey_track(id),seq INTEGER NOT NULL,segment INTEGER NOT NULL,utc_ms INTEGER NOT NULL,fix_json TEXT NOT NULL,PRIMARY KEY(track_id,seq));
UPDATE ovg_local_metadata SET schema_version=2 WHERE singleton=1;
