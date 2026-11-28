CREATE TABLE ovg_edit_command(command_id TEXT PRIMARY KEY NOT NULL);
INSERT INTO ovg_edit_command SELECT command_id FROM ovg_edit_history;
UPDATE ovg_local_metadata SET schema_version=3 WHERE singleton=1;
