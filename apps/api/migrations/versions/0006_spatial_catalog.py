"""Create the project-scoped spatial reference catalog (R003)."""

from collections.abc import Sequence

from alembic import op

revision: str = "0006_spatial_catalog"
down_revision: str | Sequence[str] | None = "0005_retire_foundation_sample"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

TABLES = ("data_sources", "source_fields", "reference_features", "layers", "catalog_changes")

# Frozen database vocabulary: never import the live spatial domain in a migration.
_SCOPE = (
    "organization_id = NULLIF(current_setting('openvietgis.organization_id', true), '')::uuid "
    "AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid"
)


def _create_validation_functions() -> None:
    # SQL-only immutable validators enforce JSON container, field and scalar shape.
    # More detailed schema interpretation and geodetic evidence are checked in the
    # domain and adapter; the database never invents or reprojects native metadata.
    op.execute("""
        CREATE FUNCTION spatial.valid_revision(value_json jsonb, source_change boolean)
        RETURNS boolean LANGUAGE sql IMMUTABLE SET search_path = pg_catalog AS $$
            SELECT COALESCE(CASE WHEN jsonb_typeof(value_json) = 'object' THEN
                CASE WHEN source_change THEN
                    value_json ?& ARRAY['catalog_revision','content_revision','schema_revision']
                    AND value_json - ARRAY['catalog_revision','content_revision','schema_revision']
                        = '{}'::jsonb
                    AND jsonb_typeof(value_json->'content_revision') = 'string'
                    AND (value_json->>'content_revision') ~ '^[a-f0-9]{64}$'
                    AND jsonb_typeof(value_json->'schema_revision') = 'string'
                    AND (value_json->>'schema_revision') ~ '^[a-f0-9]{64}$'
                    AND CASE WHEN jsonb_typeof(value_json->'catalog_revision') = 'number'
                        THEN (value_json->>'catalog_revision')::numeric
                                BETWEEN 1 AND 9007199254740991
                            AND trunc((value_json->>'catalog_revision')::numeric)
                                = (value_json->>'catalog_revision')::numeric
                        ELSE false END
                ELSE
                    value_json ? 'layer_revision'
                    AND value_json - 'layer_revision' = '{}'::jsonb
                    AND CASE WHEN jsonb_typeof(value_json->'layer_revision') = 'number'
                        THEN (value_json->>'layer_revision')::numeric
                                BETWEEN 1 AND 9007199254740991
                            AND trunc((value_json->>'layer_revision')::numeric)
                                = (value_json->>'layer_revision')::numeric
                        ELSE false END
                END ELSE false END, false)
        $$
    """)
    op.execute("""
        CREATE FUNCTION spatial.valid_change_values(
            entry_list jsonb, source_change boolean, creation boolean
        ) RETURNS boolean LANGUAGE sql IMMUTABLE SET search_path = pg_catalog AS $$
            SELECT COALESCE(CASE WHEN jsonb_typeof(entry_list) = 'array' THEN
                jsonb_array_length(entry_list) BETWEEN 1 AND 100
                AND (
                    SELECT count(*) = count(DISTINCT item->>'field')
                        AND bool_and(COALESCE(CASE WHEN jsonb_typeof(item) = 'object' THEN
                            item ?& ARRAY['field','old_value','new_value']
                            AND item - ARRAY['field','old_value','new_value'] = '{}'::jsonb
                            AND jsonb_typeof(item->'field') = 'string'
                            AND length(item->>'field') BETWEEN 1 AND 100
                            AND CASE WHEN source_change THEN
                                item->>'field' IN (
                                    'name','kind','reference_key','fixture_version',
                                    'manifest_digest','source_asset_id','lifecycle',
                                    'validation_state','validation_reason',
                                    'validation_reason.code','validation_reason.message',
                                    'source_crs','stored_crs','schema_state','geometry_family',
                                    'dimensions','feature_count_state','feature_count',
                                    'extent_state','catalog_revision','content_revision',
                                    'schema_revision'
                                ) OR (item->>'field') ~
                                    '^fields[.][A-Za-z][A-Za-z0-9_]{0,63}[.](name|type|nullable|ordinal)$'
                            ELSE item->>'field' IN ('name','source_id','lifecycle','layer_revision')
                            END
                            AND jsonb_typeof(item->'old_value') IN
                                ('string','number','boolean','null')
                            AND jsonb_typeof(item->'new_value') IN
                                ('string','number','boolean','null')
                            AND COALESCE(length(item->>'old_value'), 0) <= 500
                            AND COALESCE(length(item->>'new_value'), 0) <= 500
                            AND item->'old_value' <> item->'new_value'
                            AND (NOT creation OR item->'old_value' = 'null'::jsonb)
                        ELSE false END, false))
                    FROM jsonb_array_elements(entry_list) AS item
                )
                ELSE false END, false)
        $$
    """)
    op.execute("""
        CREATE FUNCTION spatial.valid_attributes(values_json jsonb)
        RETURNS boolean LANGUAGE sql IMMUTABLE SET search_path = pg_catalog AS $$
            SELECT COALESCE(CASE WHEN jsonb_typeof(values_json) = 'object' THEN
                (
                    SELECT count(*) <= 100 AND COALESCE(bool_and(
                        key ~ '^[A-Za-z][A-Za-z0-9_]{0,63}$'
                        AND CASE jsonb_typeof(value)
                            WHEN 'string' THEN length(value #>> '{}') <= 4096
                            WHEN 'number' THEN
                                (value #>> '{}')::numeric BETWEEN -2147483648 AND 2147483647
                                AND trunc((value #>> '{}')::numeric) = (value #>> '{}')::numeric
                            WHEN 'boolean' THEN true
                            WHEN 'null' THEN true
                            ELSE false
                        END
                    ), true)
                    FROM jsonb_each(values_json)
                ) ELSE false END, false)
        $$
    """)
    op.execute("""
        CREATE FUNCTION spatial.valid_extent(value_json jsonb, native_code integer)
        RETURNS boolean LANGUAGE sql IMMUTABLE SET search_path = pg_catalog AS $$
            SELECT COALESCE(CASE WHEN
                jsonb_typeof(value_json) = 'object'
                AND native_code IN (4326,3857,4756,5899)
                AND value_json ?& ARRAY['min_x','min_y','max_x','max_y','crs_code']
                AND value_json - ARRAY['min_x','min_y','max_x','max_y','crs_code'] = '{}'::jsonb
                AND jsonb_typeof(value_json->'crs_code') = 'number'
                AND jsonb_typeof(value_json->'min_x') = 'number'
                AND jsonb_typeof(value_json->'min_y') = 'number'
                AND jsonb_typeof(value_json->'max_x') = 'number'
                AND jsonb_typeof(value_json->'max_y') = 'number'
            THEN
                (value_json->>'crs_code')::numeric = native_code
                AND (value_json->>'min_x')::numeric <= (value_json->>'max_x')::numeric
                AND (value_json->>'min_y')::numeric <= (value_json->>'max_y')::numeric
                AND (value_json->>'min_x')::numeric >= -1.7976931348623157e308
                AND (value_json->>'max_x')::numeric <= 1.7976931348623157e308
                AND (value_json->>'min_y')::numeric >= -1.7976931348623157e308
                AND (value_json->>'max_y')::numeric <= 1.7976931348623157e308
                AND CASE WHEN native_code IN (4326,4756) THEN
                    (value_json->>'min_x')::numeric >= -180
                    AND (value_json->>'max_x')::numeric <= 180
                    AND (value_json->>'min_y')::numeric >= -90
                    AND (value_json->>'max_y')::numeric <= 90
                WHEN native_code = 3857 THEN
                    (value_json->>'min_x')::numeric >= -20037508.342789244
                    AND (value_json->>'max_x')::numeric <= 20037508.342789244
                    AND (value_json->>'min_y')::numeric >= -20037508.342789244
                    AND (value_json->>'max_y')::numeric <= 20037508.342789244
                ELSE true END
            ELSE false END, false)
        $$
    """)
    for signature in (
        "valid_revision(jsonb, boolean)",
        "valid_change_values(jsonb, boolean, boolean)",
        "valid_attributes(jsonb)",
        "valid_extent(jsonb, integer)",
    ):
        op.execute(f"REVOKE ALL ON FUNCTION spatial.{signature} FROM PUBLIC")
        op.execute(f"GRANT EXECUTE ON FUNCTION spatial.{signature} TO openvietgis_runtime")


def upgrade() -> None:
    op.execute("CREATE SCHEMA spatial")
    op.execute("REVOKE ALL ON SCHEMA spatial FROM PUBLIC")
    op.execute("GRANT USAGE ON SCHEMA spatial TO openvietgis_runtime")
    _create_validation_functions()
    op.create_unique_constraint(
        "uq_audit_events_organization_id_id",
        "audit_events",
        ["organization_id", "id"],
        schema="audit",
    )
    op.execute("""
        CREATE TABLE spatial.data_sources (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            name varchar(200) NOT NULL,
            kind varchar(32) NOT NULL,
            reference_key varchar(100) NOT NULL,
            fixture_version varchar(100) NOT NULL,
            manifest_digest varchar(64) NOT NULL,
            source_asset_id uuid,
            lifecycle varchar(32) NOT NULL,
            validation_state varchar(32) NOT NULL,
            validation_reason jsonb,
            source_crs integer,
            stored_crs integer,
            schema_state varchar(32) NOT NULL,
            geometry_family varchar(32) NOT NULL,
            dimensions varchar(32) NOT NULL,
            feature_count_state varchar(32) NOT NULL,
            feature_count integer,
            extent_state varchar(32) NOT NULL,
            extent jsonb,
            catalog_revision bigint NOT NULL,
            content_revision varchar(64) NOT NULL,
            schema_revision varchar(64) NOT NULL,
            created_at timestamptz NOT NULL,
            updated_at timestamptz NOT NULL,
            retired_at timestamptz,
            CONSTRAINT fk_sources_project FOREIGN KEY (organization_id, project_id)
                REFERENCES workspace.projects (organization_id, id) ON DELETE RESTRICT,
            CONSTRAINT uq_sources_scope UNIQUE (organization_id, project_id, id),
            CONSTRAINT uq_sources_reference_key UNIQUE (organization_id, project_id, reference_key),
            CONSTRAINT uq_sources_schema_pin
                UNIQUE (organization_id, project_id, id, schema_revision),
            CONSTRAINT uq_sources_content_pins
                UNIQUE (organization_id, project_id, id, content_revision, schema_revision),
            CONSTRAINT ck_source_name CHECK (name = btrim(name) AND length(name) >= 1),
            CONSTRAINT ck_source_kind CHECK (kind='reference_fixture' AND source_asset_id IS NULL),
            CONSTRAINT ck_source_reference_key CHECK (reference_key ~ '^[a-z0-9_]{1,100}$'),
            CONSTRAINT ck_source_fixture_version CHECK (length(fixture_version) >= 1),
            CONSTRAINT ck_source_hashes CHECK (
                manifest_digest ~ '^[a-f0-9]{64}$'
                AND content_revision ~ '^[a-f0-9]{64}$'
                AND schema_revision ~ '^[a-f0-9]{64}$'
            ),
            CONSTRAINT ck_source_catalog_revision CHECK (
                catalog_revision BETWEEN 1 AND 9007199254740991
            ),
            CONSTRAINT ck_source_lifecycle CHECK (
                lifecycle IN ('active','retired')
                AND (lifecycle='active') = (retired_at IS NULL)
                AND created_at <= updated_at
                AND (retired_at IS NULL OR retired_at BETWEEN created_at AND updated_at)
            ),
            CONSTRAINT ck_source_validation_state CHECK (
                validation_state IN ('ready','invalid','not_ready')
                AND (validation_state='ready') = (validation_reason IS NULL)
            ),
            CONSTRAINT ck_source_validation_reason CHECK (
                validation_reason IS NULL OR COALESCE(
                    jsonb_typeof(validation_reason)='object'
                    AND validation_reason ?& ARRAY['code','message']
                    AND validation_reason - ARRAY['code','message'] = '{}'::jsonb
                    AND jsonb_typeof(validation_reason->'code')='string'
                    AND jsonb_typeof(validation_reason->'message')='string'
                    AND validation_reason->>'code' IN (
                        'MISSING_CRS','UNKNOWN_CRS','UNSUPPORTED_CRS','CRS_MISMATCH',
                        'GEOMETRY_FAMILY_MISMATCH','UNSUPPORTED_DIMENSIONS','UNSUPPORTED_GEOMETRY',
                        'NULL_GEOMETRY','EMPTY_GEOMETRY','INVALID_GEOMETRY','AXIS_CONFLICT',
                        'COORDINATE_OUT_OF_DOMAIN','EXTENT_WRAP_UNSUPPORTED','SCHEMA_MISMATCH',
                        'FIXTURE_UNAVAILABLE','BOUNDS_EXCEEDED'
                    )
                    AND length(validation_reason->>'message') BETWEEN 1 AND 500,
                    false
                )
            ),
            CONSTRAINT ck_source_crs CHECK (
                source_crs IN (4326,3857,4756,5899,3405,3406) OR source_crs IS NULL
            ),
            CONSTRAINT ck_stored_crs CHECK (
                stored_crs IN (4326,3857,4756,5899,3405,3406) OR stored_crs IS NULL
            ),
            CONSTRAINT ck_source_schema_state CHECK (schema_state IN ('known','unavailable')),
            CONSTRAINT ck_source_geometry_family CHECK (
                geometry_family IN ('Point','LineString','Polygon','mixed','unsupported','unknown')
            ),
            CONSTRAINT ck_source_dimensions CHECK (
                dimensions IN ('XY','XYZ','XYM','XYZM','mixed','unknown')
            ),
            CONSTRAINT ck_source_count CHECK (
                feature_count_state IN ('known','unavailable')
                AND (feature_count_state='known') = (feature_count IS NOT NULL)
                AND (feature_count IS NULL OR feature_count >= 0)
            ),
            CONSTRAINT ck_source_extent CHECK (
                extent_state IN ('available','empty','unavailable')
                AND (extent_state='available') = (extent IS NOT NULL)
                AND (extent IS NULL OR spatial.valid_extent(extent, stored_crs))
            ),
            CONSTRAINT ck_source_ready_metadata CHECK (COALESCE(
                CASE WHEN validation_state='ready' THEN
                    source_crs IN (4326,3857,4756,5899)
                    AND source_crs=stored_crs
                    AND schema_state='known'
                    AND geometry_family IN ('Point','LineString','Polygon')
                    AND dimensions='XY'
                    AND feature_count_state='known'
                    AND extent_state=CASE WHEN feature_count=0 THEN 'empty' ELSE 'available' END
                ELSE extent_state='unavailable' END, false
            ))
        )
    """)
    op.execute("""
        CREATE TABLE spatial.source_fields (
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            source_id uuid NOT NULL,
            field_id varchar(64) COLLATE "C" NOT NULL,
            name varchar(200) NOT NULL,
            ordinal integer NOT NULL,
            type varchar(32) NOT NULL,
            nullable boolean NOT NULL,
            schema_revision varchar(64) NOT NULL,
            PRIMARY KEY (organization_id, project_id, source_id, field_id),
            CONSTRAINT fk_fields_source FOREIGN KEY (organization_id, project_id, source_id)
                REFERENCES spatial.data_sources (organization_id, project_id, id)
                ON DELETE RESTRICT,
            CONSTRAINT fk_fields_pin FOREIGN KEY (
                organization_id, project_id, source_id, schema_revision
            ) REFERENCES spatial.data_sources (
                organization_id, project_id, id, schema_revision
            ) DEFERRABLE INITIALLY DEFERRED,
            CONSTRAINT uq_fields_ordinal UNIQUE (organization_id, project_id, source_id, ordinal),
            CONSTRAINT ck_field_id CHECK (field_id ~ '^[A-Za-z][A-Za-z0-9_]{0,63}$'),
            CONSTRAINT ck_field_name CHECK (name=btrim(name) AND length(name) >= 1),
            CONSTRAINT ck_field_ordinal CHECK (ordinal BETWEEN 0 AND 99),
            CONSTRAINT ck_field_type CHECK (
                type IN ('text','integer','decimal','boolean','timestamp')
            ),
            CONSTRAINT ck_field_schema_revision CHECK (schema_revision ~ '^[a-f0-9]{64}$')
        )
    """)
    op.execute("""
        CREATE TABLE spatial.reference_features (
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            source_id uuid NOT NULL,
            feature_id varchar(128) COLLATE "C" NOT NULL,
            attributes jsonb NOT NULL,
            geometry geometry,
            geometry_state varchar(32) NOT NULL,
            content_revision varchar(64) NOT NULL,
            schema_revision varchar(64) NOT NULL,
            PRIMARY KEY (organization_id, project_id, source_id, feature_id),
            CONSTRAINT fk_features_source FOREIGN KEY (organization_id, project_id, source_id)
                REFERENCES spatial.data_sources (organization_id, project_id, id)
                ON DELETE RESTRICT,
            CONSTRAINT fk_features_pins FOREIGN KEY (
                organization_id, project_id, source_id, content_revision, schema_revision
            ) REFERENCES spatial.data_sources (
                organization_id, project_id, id, content_revision, schema_revision
            ) DEFERRABLE INITIALLY DEFERRED,
            CONSTRAINT ck_feature_id CHECK (feature_id ~ '^[A-Za-z0-9_-]{1,128}$'),
            CONSTRAINT ck_feature_attributes CHECK (spatial.valid_attributes(attributes)),
            CONSTRAINT ck_feature_hashes CHECK (
                content_revision ~ '^[a-f0-9]{64}$' AND schema_revision ~ '^[a-f0-9]{64}$'
            ),
            CONSTRAINT ck_feature_geometry_state CHECK (
                geometry_state IN ('present','absent','empty','invalid','unsupported')
                AND (geometry_state='absent') = (geometry IS NULL)
                AND (geometry IS NULL OR (geometry_state='empty') = ST_IsEmpty(geometry))
                AND (geometry_state <> 'present' OR (
                    ST_GeometryType(geometry) IN ('ST_Point','ST_LineString','ST_Polygon')
                    AND ST_NDims(geometry)=2 AND ST_NPoints(geometry)<=4096
                    AND ST_IsValid(geometry)
                ))
                AND (geometry_state <> 'unsupported' OR (
                    ST_NDims(geometry)<>2 OR ST_GeometryType(geometry) NOT IN
                        ('ST_Point','ST_LineString','ST_Polygon')
                ))
            )
        )
    """)
    op.execute("""
        CREATE TABLE spatial.layers (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            source_id uuid NOT NULL,
            name varchar(200) NOT NULL,
            lifecycle varchar(32) NOT NULL,
            layer_revision bigint NOT NULL,
            created_at timestamptz NOT NULL,
            updated_at timestamptz NOT NULL,
            retired_at timestamptz,
            CONSTRAINT uq_layers_scope UNIQUE (organization_id, project_id, id),
            CONSTRAINT fk_layers_project FOREIGN KEY (organization_id, project_id)
                REFERENCES workspace.projects (organization_id, id) ON DELETE RESTRICT,
            CONSTRAINT fk_layers_source FOREIGN KEY (organization_id, project_id, source_id)
                REFERENCES spatial.data_sources (organization_id, project_id, id)
                ON DELETE RESTRICT,
            CONSTRAINT ck_layer_name CHECK (name=btrim(name) AND length(name) >= 1),
            CONSTRAINT ck_layer_revision CHECK (layer_revision BETWEEN 1 AND 9007199254740991),
            CONSTRAINT ck_layer_lifecycle CHECK (
                lifecycle IN ('active','retired')
                AND (lifecycle='active') = (retired_at IS NULL)
                AND created_at <= updated_at
                AND (retired_at IS NULL OR retired_at BETWEEN created_at AND updated_at)
            )
        )
    """)
    op.execute("""
        CREATE INDEX ix_layers_source_lifecycle
        ON spatial.layers (organization_id, project_id, source_id, lifecycle)
    """)
    op.execute("""
        CREATE TABLE spatial.catalog_changes (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            resource_type varchar(32) NOT NULL,
            resource_id uuid NOT NULL,
            source_id uuid,
            layer_id uuid,
            actor_type varchar(32) NOT NULL,
            actor_id uuid NOT NULL,
            actor_display_snapshot varchar(200) NOT NULL,
            operation varchar(48) NOT NULL,
            created_at timestamptz NOT NULL,
            audit_event_id uuid NOT NULL,
            prior_revision jsonb,
            new_revision jsonb NOT NULL,
            changes jsonb NOT NULL,
            CONSTRAINT uq_changes_scope UNIQUE (organization_id, project_id, id),
            CONSTRAINT fk_changes_project FOREIGN KEY (organization_id, project_id)
                REFERENCES workspace.projects (organization_id, id) ON DELETE RESTRICT,
            CONSTRAINT fk_changes_source FOREIGN KEY (organization_id, project_id, source_id)
                REFERENCES spatial.data_sources (organization_id, project_id, id)
                ON DELETE RESTRICT,
            CONSTRAINT fk_changes_layer FOREIGN KEY (organization_id, project_id, layer_id)
                REFERENCES spatial.layers (organization_id, project_id, id)
                ON DELETE RESTRICT,
            CONSTRAINT fk_changes_audit FOREIGN KEY (organization_id, audit_event_id)
                REFERENCES audit.audit_events (organization_id, id) ON DELETE RESTRICT,
            CONSTRAINT ck_change_resource CHECK (COALESCE(
                (resource_type='data_source' AND source_id=resource_id AND layer_id IS NULL
                    AND operation IN (
                        'register_source','update_source','correct_source_crs','retire_source',
                        'refresh_reference_fixture'
                    )
                ) OR (
                    resource_type='layer' AND layer_id=resource_id AND source_id IS NULL
                    AND operation IN ('create_layer','rename_layer','retire_layer')
                ), false
            )),
            CONSTRAINT ck_change_actor CHECK (
                actor_type IN ('human','service_principal')
                AND actor_display_snapshot=btrim(actor_display_snapshot)
                AND length(actor_display_snapshot) >= 1
            ),
            CONSTRAINT ck_change_revisions CHECK (COALESCE(
                spatial.valid_revision(new_revision, resource_type='data_source')
                AND (prior_revision IS NULL OR
                    spatial.valid_revision(prior_revision, resource_type='data_source'))
                AND (prior_revision IS NULL) = (operation IN ('register_source','create_layer'))
                AND CASE WHEN resource_type='data_source' THEN
                    (new_revision->>'catalog_revision')::numeric =
                        COALESCE((prior_revision->>'catalog_revision')::numeric, 0) + 1
                    AND (
                        operation NOT IN ('update_source','retire_source') OR (
                            new_revision->>'content_revision' = prior_revision->>'content_revision'
                            AND new_revision->>'schema_revision' =
                                prior_revision->>'schema_revision'
                        )
                    )
                ELSE (new_revision->>'layer_revision')::numeric =
                    COALESCE((prior_revision->>'layer_revision')::numeric, 0) + 1 END,
                false
            )),
            CONSTRAINT ck_change_values CHECK (
                spatial.valid_change_values(
                    changes, resource_type='data_source', prior_revision IS NULL
                )
            )
        )
    """)
    for table in TABLES:
        op.execute(f"GRANT SELECT, INSERT ON spatial.{table} TO openvietgis_runtime")
        op.execute(f"ALTER TABLE spatial.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE spatial.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"""
            CREATE POLICY {table}_project_scope ON spatial.{table}
            FOR ALL TO openvietgis_runtime
            USING ({_SCOPE})
            WITH CHECK ({_SCOPE})
        """)
    # No DELETE, TRUNCATE, ownership/identity mutation or catalog evidence UPDATE.
    # Only catalog metadata and internally controlled feature/schema refresh may update.
    op.execute("""
        GRANT UPDATE (
            name, lifecycle, validation_state, validation_reason, source_crs, stored_crs,
            schema_state, geometry_family, dimensions, feature_count_state, feature_count,
            extent_state, extent, catalog_revision, content_revision, schema_revision,
            updated_at, retired_at
        ) ON spatial.data_sources TO openvietgis_runtime
    """)
    op.execute("""
        GRANT UPDATE (name, ordinal, type, nullable, schema_revision)
        ON spatial.source_fields TO openvietgis_runtime
    """)
    op.execute("""
        GRANT UPDATE (attributes, geometry, geometry_state, content_revision, schema_revision)
        ON spatial.reference_features TO openvietgis_runtime
    """)
    op.execute("""
        GRANT UPDATE (name, lifecycle, layer_revision, updated_at, retired_at)
        ON spatial.layers TO openvietgis_runtime
    """)


def downgrade() -> None:
    # Only the R003 catalog is removed. R002 rows, privileges and policies remain.
    for table in reversed(TABLES):
        op.drop_table(table, schema="spatial")
    for signature in (
        "valid_extent(jsonb, integer)",
        "valid_attributes(jsonb)",
        "valid_change_values(jsonb, boolean, boolean)",
        "valid_revision(jsonb, boolean)",
    ):
        op.execute(f"DROP FUNCTION spatial.{signature}")
    op.execute("DROP SCHEMA spatial")
    op.drop_constraint(
        "uq_audit_events_organization_id_id", "audit_events", schema="audit", type_="unique"
    )
