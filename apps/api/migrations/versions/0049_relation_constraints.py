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


def downgrade():
    op.execute("""
        DO $$ BEGIN
          IF EXISTS (SELECT 1 FROM editing.relations) THEN
            RAISE EXCEPTION 'Remove relation constraints explicitly before downgrade';
          END IF;
        END $$
    """)
    op.execute("DROP TABLE editing.relations")
    op.execute("DROP FUNCTION editing.relation_key(jsonb,text)")
