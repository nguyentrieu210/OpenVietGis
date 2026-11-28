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
            field_type text NOT NULL CHECK (
                field_type IN ('text','integer','decimal','boolean','timestamp')
            ),
            UNIQUE (organization_id,project_id,source_id,local_field,target_source_id,target_field),
            FOREIGN KEY (organization_id,project_id,source_id,local_field)
                REFERENCES spatial.source_fields(
                    organization_id,project_id,source_id,field_id
                ) ON DELETE RESTRICT,
            FOREIGN KEY (organization_id,project_id,target_source_id,target_field)
                REFERENCES spatial.source_fields(
                    organization_id,project_id,source_id,field_id
                ) ON DELETE RESTRICT,
            CHECK (source_id<>target_source_id OR local_field<>target_field)
        )
    """)
    op.execute(
        "CREATE INDEX relations_target ON editing.relations("
        "organization_id,project_id,target_source_id)"
    )
    op.execute("ALTER TABLE editing.relations ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE editing.relations FORCE ROW LEVEL SECURITY")
    op.execute("""
        CREATE POLICY relations_project_scope ON editing.relations FOR ALL TO openvietgis_runtime
        USING (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK (
          organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
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
        LANGUAGE plpgsql SET search_path=pg_catalog AS $$
        BEGIN
          IF TG_TABLE_NAME='source_fields' THEN
            IF NEW.type IS DISTINCT FROM OLD.type AND EXISTS (
              SELECT 1 FROM editing.relations r
              WHERE r.organization_id=OLD.organization_id AND r.project_id=OLD.project_id
              AND ((r.source_id=OLD.source_id AND r.local_field=OLD.field_id)
                OR (r.target_source_id=OLD.source_id AND r.target_field=OLD.field_id))
            ) THEN
              RAISE EXCEPTION USING ERRCODE='OV409',
                MESSAGE='Relation field type is locked';
            END IF;
          ELSE
            IF (NEW.lifecycle<>'active' OR NEW.validation_state<>'ready') AND EXISTS (
              SELECT 1 FROM editing.relations r
              WHERE r.organization_id=OLD.organization_id AND r.project_id=OLD.project_id
              AND (r.source_id=OLD.id OR r.target_source_id=OLD.id)
            ) THEN
              RAISE EXCEPTION USING ERRCODE='OV409',
                MESSAGE='Source has an active relation constraint';
            END IF;
          END IF;
          RETURN NEW;
        END $$
    """)
    op.execute("""
        CREATE TRIGGER relation_field_type_guard
        BEFORE UPDATE OF type ON spatial.source_fields
        FOR EACH ROW EXECUTE FUNCTION editing.guard_relation_metadata()
    """)
    op.execute("""
        CREATE TRIGGER relation_source_state_guard
        BEFORE UPDATE OF lifecycle,validation_state ON spatial.data_sources
        FOR EACH ROW EXECUTE FUNCTION editing.guard_relation_metadata()
    """)

    # All feature writes serialize with relation-definition edits on the existing project row.
    # Revision-only and geometry-only updates cannot change relation keys.
    op.execute("""
        CREATE FUNCTION editing.lock_relation_write() RETURNS trigger
        LANGUAGE plpgsql SET search_path=pg_catalog AS $$
        DECLARE org uuid; project uuid; source uuid;
        BEGIN
          IF TG_OP='UPDATE' THEN
            IF NEW.attributes IS NOT DISTINCT FROM OLD.attributes THEN RETURN NEW; END IF;
          END IF;
          IF TG_OP='DELETE' THEN
            org:=OLD.organization_id; project:=OLD.project_id; source:=OLD.source_id;
          ELSE
            org:=NEW.organization_id; project:=NEW.project_id; source:=NEW.source_id;
          END IF;
          PERFORM 1 FROM workspace.projects p
            WHERE p.organization_id=org AND p.id=project FOR UPDATE;
          IF NOT FOUND THEN
            RAISE EXCEPTION USING ERRCODE='OV409', MESSAGE='Relation write scope unavailable';
          END IF;
          IF current_user='openvietgis_runtime' AND gis_access.scoped(org,project)
            AND EXISTS (SELECT 1 FROM editing.relations r
               WHERE r.organization_id=org AND r.project_id=project
               AND (r.source_id=source OR r.target_source_id=source))
          THEN
            RAISE EXCEPTION USING ERRCODE='OV409',
              MESSAGE='Relation-bound writes require ' || 'unrestricted project access';
          END IF;
          IF TG_OP='DELETE' THEN RETURN OLD; END IF;
          RETURN NEW;
        END $$
    """)
    op.execute("""
        CREATE TRIGGER relation_write_lock
        BEFORE INSERT OR UPDATE OF attributes OR DELETE ON spatial.reference_features
        FOR EACH ROW EXECUTE FUNCTION editing.lock_relation_write()
    """)

    # Evaluate only the changed key against the final transaction state, so valid
    # multi-operation edits and deletes/reinserts are not rejected midway.
    op.execute("""
        CREATE FUNCTION editing.assert_relation_key(rule editing.relations, k text)
        RETURNS void LANGUAGE plpgsql SET search_path=pg_catalog AS $$
        DECLARE n integer;
        BEGIN
          IF k IS NULL THEN RETURN; END IF;
          SELECT count(*) INTO n FROM (
            SELECT 1 FROM spatial.reference_features f
            WHERE (f.organization_id,f.project_id,f.source_id)=
                  (rule.organization_id,rule.project_id,rule.target_source_id)
              AND editing.relation_key(
                    f.attributes->rule.target_field,rule.field_type
                  )=k COLLATE "C"
            LIMIT 2
          ) target;
          IF (rule.cardinality='one' AND n>1)
             OR (n=0 AND EXISTS (
                SELECT 1 FROM spatial.reference_features f
                WHERE (f.organization_id,f.project_id,f.source_id)=
                      (rule.organization_id,rule.project_id,rule.source_id)
                  AND editing.relation_key(
                        f.attributes->rule.local_field,rule.field_type
                      )=k COLLATE "C"
             ))
          THEN
            RAISE EXCEPTION USING ERRCODE='OV409', MESSAGE='Relation constraint would be violated';
          END IF;
        END $$
    """)
    op.execute("""
        CREATE FUNCTION editing.check_relation_write() RETURNS trigger
        LANGUAGE plpgsql SET search_path=pg_catalog AS $$
        DECLARE org uuid; project uuid; source uuid; rule editing.relations;
                before_attrs jsonb; after_attrs jsonb; before_key text; after_key text;
        BEGIN
          IF TG_OP='DELETE' THEN
            org:=OLD.organization_id; project:=OLD.project_id; source:=OLD.source_id;
            before_attrs:=OLD.attributes;
          ELSE
            org:=NEW.organization_id; project:=NEW.project_id; source:=NEW.source_id;
            after_attrs:=NEW.attributes;
            IF TG_OP='UPDATE' THEN before_attrs:=OLD.attributes; END IF;
          END IF;
          FOR rule IN SELECT r.* FROM editing.relations r
            WHERE r.organization_id=org AND r.project_id=project
              AND (r.source_id=source OR r.target_source_id=source)
          LOOP
            IF rule.source_id=source THEN
              before_key:=editing.relation_key(before_attrs->rule.local_field,rule.field_type);
              after_key:=editing.relation_key(after_attrs->rule.local_field,rule.field_type);
              IF before_key IS DISTINCT FROM after_key THEN
                PERFORM editing.assert_relation_key(rule,after_key);
              END IF;
            END IF;
            IF rule.target_source_id=source THEN
              before_key:=editing.relation_key(before_attrs->rule.target_field,rule.field_type);
              after_key:=editing.relation_key(after_attrs->rule.target_field,rule.field_type);
              IF before_key IS DISTINCT FROM after_key THEN
                PERFORM editing.assert_relation_key(rule,before_key);
                PERFORM editing.assert_relation_key(rule,after_key);
              END IF;
            END IF;
          END LOOP;
          RETURN NULL;
        END $$
    """)
    op.execute("""
        CREATE CONSTRAINT TRIGGER relation_feature_insert_delete
        AFTER INSERT OR DELETE ON spatial.reference_features
        DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
        EXECUTE FUNCTION editing.check_relation_write()
    """)
    op.execute("""
        CREATE CONSTRAINT TRIGGER relation_feature_update
        AFTER UPDATE ON spatial.reference_features
        DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
        WHEN (OLD.attributes IS DISTINCT FROM NEW.attributes)
        EXECUTE FUNCTION editing.check_relation_write()
    """)
    # Definition changes from any SQL write path use the same project lock and
    # validate both fields plus the entire existing key population.
    op.execute("""
        CREATE FUNCTION editing.guard_relation_definition() RETURNS trigger
        LANGUAGE plpgsql SET search_path=pg_catalog AS $$
        DECLARE org uuid; project uuid; field_count integer; invalid boolean;
        BEGIN
          IF TG_OP='DELETE' THEN org:=OLD.organization_id; project:=OLD.project_id;
          ELSE org:=NEW.organization_id; project:=NEW.project_id; END IF;
          PERFORM 1 FROM workspace.projects p
            WHERE p.organization_id=org AND p.id=project FOR UPDATE;
          IF NOT FOUND THEN
            RAISE EXCEPTION USING ERRCODE='OV409',
              MESSAGE='Relation project unavailable';
          END IF;
          IF current_user='openvietgis_runtime' AND gis_access.scoped(org,project)
          THEN
            RAISE EXCEPTION USING ERRCODE='OV409',
              MESSAGE='Relation management requires ' || 'unrestricted project access';
          END IF;
          IF TG_OP='DELETE' THEN RETURN OLD; END IF;
          IF TG_OP='UPDATE' THEN
            RAISE EXCEPTION USING ERRCODE='OV409',
              MESSAGE='Replace relation definitions explicitly';
          END IF;
          SELECT count(*) INTO field_count FROM spatial.source_fields f
          JOIN spatial.data_sources s
            ON (s.organization_id,s.project_id,s.id)=
               (f.organization_id,f.project_id,f.source_id)
          WHERE f.organization_id=org AND f.project_id=project
            AND s.lifecycle='active' AND s.validation_state='ready'
            AND f.schema_revision=s.schema_revision AND f.type=NEW.field_type
            AND ((f.source_id=NEW.source_id AND f.field_id=NEW.local_field)
              OR (f.source_id=NEW.target_source_id AND f.field_id=NEW.target_field));
          IF field_count<>2 THEN
            RAISE EXCEPTION USING ERRCODE='OV409',
              MESSAGE='Relation fields unavailable or incompatible';
          END IF;
          IF (
            SELECT count(*) FROM editing.relations r
            WHERE r.organization_id=org AND r.project_id=project
          ) >= 100 THEN
            RAISE EXCEPTION USING ERRCODE='OV409', MESSAGE='Relation definition limit exceeded';
          END IF;
          WITH targets AS (
            SELECT editing.relation_key(
                f.attributes->NEW.target_field,NEW.field_type
            ) COLLATE "C" AS k,count(*) AS n
            FROM spatial.reference_features f
            WHERE (f.organization_id,f.project_id,f.source_id)=(org,project,NEW.target_source_id)
            GROUP BY 1
          ), locals AS (
            SELECT DISTINCT editing.relation_key(
                f.attributes->NEW.local_field,NEW.field_type
            ) COLLATE "C" AS k
            FROM spatial.reference_features f
            WHERE (f.organization_id,f.project_id,f.source_id)=(org,project,NEW.source_id)
          )
          SELECT EXISTS(
              SELECT 1 FROM locals l LEFT JOIN targets t ON t.k=l.k
              WHERE l.k IS NOT NULL AND t.k IS NULL
          )
            OR (
                NEW.cardinality='one'
                AND EXISTS(SELECT 1 FROM targets t WHERE t.k IS NOT NULL AND t.n>1)
            )
            INTO invalid;
          IF invalid THEN
            RAISE EXCEPTION USING ERRCODE='OV409',
              MESSAGE='Relation constraint would be violated';
          END IF;
          RETURN NEW;
        END $$
    """)
    op.execute("""
        CREATE TRIGGER relation_definition_guard
        BEFORE INSERT OR UPDATE OR DELETE ON editing.relations
        FOR EACH ROW EXECUTE FUNCTION editing.guard_relation_definition()
    """)
    op.execute(
        "REVOKE ALL ON FUNCTION editing.assert_relation_key("
        "editing.relations,text) FROM PUBLIC"
    )
    op.execute(
        "GRANT EXECUTE ON FUNCTION editing.assert_relation_key("
        "editing.relations,text) TO openvietgis_runtime"
    )


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
    op.execute("DROP TRIGGER relation_feature_update ON spatial.reference_features")
    op.execute("DROP TRIGGER relation_feature_insert_delete ON spatial.reference_features")
    op.execute("DROP TRIGGER relation_write_lock ON spatial.reference_features")
    op.execute("DROP FUNCTION editing.check_relation_write()")
    op.execute("DROP FUNCTION editing.lock_relation_write()")
    op.execute("DROP FUNCTION editing.assert_relation_key(editing.relations,text)")
    op.execute("DROP TRIGGER relation_definition_guard ON editing.relations")
    op.execute("DROP FUNCTION editing.guard_relation_definition()")
    op.execute("DROP TABLE editing.relations")
    op.execute("DROP FUNCTION editing.relation_key(jsonb,text)")
