"""Project-scoped persistent relation constraints for canonical changesets."""
from alembic import op

revision = "0049_relation_constraints"
down_revision = "0048_processing_extended"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
        CREATE TABLE editing.relations (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            source_id uuid NOT NULL,
            local_field varchar(64) COLLATE "C" NOT NULL,
            target_source_id uuid NOT NULL,
            target_field varchar(64) COLLATE "C" NOT NULL,
            cardinality text NOT NULL CHECK (cardinality IN ('one','many')),
            field_type text NOT NULL CHECK (field_type IN ('text','integer','decimal','boolean','timestamp')),
            UNIQUE (organization_id,project_id,source_id,local_field,target_source_id,target_field),
            FOREIGN KEY (organization_id,project_id,source_id,local_field)
                REFERENCES spatial.source_fields(organization_id,project_id,source_id,field_id) ON DELETE RESTRICT,
            FOREIGN KEY (organization_id,project_id,target_source_id,target_field)
                REFERENCES spatial.source_fields(organization_id,project_id,source_id,field_id) ON DELETE RESTRICT,
            CHECK (source_id<>target_source_id OR local_field<>target_field)
        )
    """)
    op.execute("CREATE INDEX relations_target ON editing.relations(organization_id,project_id,target_source_id)")
    op.execute("ALTER TABLE editing.relations ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE editing.relations FORCE ROW LEVEL SECURITY")
    op.execute("""
        CREATE POLICY relations_project_scope ON editing.relations FOR ALL TO openvietgis_runtime
        USING (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
    """)
    op.execute("GRANT SELECT,INSERT,DELETE ON editing.relations TO openvietgis_runtime")
    op.execute("""
        CREATE FUNCTION editing.relation_key(v jsonb, kind text) RETURNS text
        LANGUAGE sql IMMUTABLE STRICT SET search_path=pg_catalog AS $$
          SELECT CASE WHEN v='null'::jsonb THEN NULL
            WHEN kind IN ('integer','decimal') THEN trim_scale((v #>> '{}')::numeric)::text
            ELSE v #>> '{}' END
        $$
    """)
    op.execute("REVOKE ALL ON FUNCTION editing.relation_key(jsonb,text) FROM PUBLIC")
    op.execute("GRANT EXECUTE ON FUNCTION editing.relation_key(jsonb,text) TO openvietgis_runtime")
    # Prevent removal/type changes from invalidating a persistent definition.
    # Metadata guards do not expose the related layer or key in their error.
    op.execute("""
        CREATE FUNCTION editing.guard_relation_metadata() RETURNS trigger
        LANGUAGE plpgsql SET search_path=pg_catalog AS $
        BEGIN
          IF TG_TABLE_NAME='source_fields' THEN
            IF NEW.type IS DISTINCT FROM OLD.type AND EXISTS (
              SELECT 1 FROM editing.relations r
              WHERE r.organization_id=OLD.organization_id AND r.project_id=OLD.project_id
              AND ((r.source_id=OLD.source_id AND r.local_field=OLD.field_id)
                OR (r.target_source_id=OLD.source_id AND r.target_field=OLD.field_id))
            ) THEN RAISE EXCEPTION USING ERRCODE='OV409', MESSAGE='Relation field type is locked'; END IF;
          ELSE
            IF (NEW.lifecycle<>'active' OR NEW.validation_state<>'ready') AND EXISTS (
              SELECT 1 FROM editing.relations r
              WHERE r.organization_id=OLD.organization_id AND r.project_id=OLD.project_id
              AND (r.source_id=OLD.id OR r.target_source_id=OLD.id)
            ) THEN RAISE EXCEPTION USING ERRCODE='OV409', MESSAGE='Source has an active relation constraint'; END IF;
          END IF;
          RETURN NEW;
        END $
    """)
    op.execute("CREATE TRIGGER relation_field_type_guard BEFORE UPDATE OF type ON spatial.source_fields FOR EACH ROW EXECUTE FUNCTION editing.guard_relation_metadata()")
    op.execute("CREATE TRIGGER relation_source_state_guard BEFORE UPDATE OF lifecycle,validation_state ON spatial.data_sources FOR EACH ROW EXECUTE FUNCTION editing.guard_relation_metadata()")


def downgrade():
    op.execute("""
        DO $$ BEGIN
          IF EXISTS (SELECT 1 FROM editing.relations) THEN
            RAISE EXCEPTION 'Remove relation constraints explicitly before downgrade';
          END IF;
        END $$
    """)
    op.execute("DROP TRIGGER relation_field_type_guard ON spatial.source_fields")
    op.execute("DROP TRIGGER relation_source_state_guard ON spatial.data_sources")
    op.execute("DROP FUNCTION editing.guard_relation_metadata()")
    op.execute("DROP TABLE editing.relations")
    op.execute("DROP FUNCTION editing.relation_key(jsonb,text)")
