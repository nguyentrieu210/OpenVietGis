"""Spec 051: reviewer sign-off of a reconciled hamlet / road and the lock it puts on survey points."""

from alembic import op

revision = "hn_0011_survey_signoffs"
down_revision = "hn_0010_address_verification"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE house_numbering.survey_signoffs(
        id uuid PRIMARY KEY, organization_id uuid NOT NULL, project_id uuid NOT NULL,
        source_id uuid NOT NULL, ap varchar(200) NOT NULL CHECK(length(btrim(ap))>0),
        ten_duong varchar(300) CHECK(ten_duong IS NULL OR length(btrim(ten_duong))>0),
        point_count integer NOT NULL CHECK(point_count>0),
        signed_by_actor_id uuid NOT NULL, signed_at timestamptz NOT NULL DEFAULT now(),
        note varchar(1000),
        revoked_by_actor_id uuid, revoked_at timestamptz, revoke_reason varchar(1000),
        CHECK((revoked_at IS NULL)=(revoked_by_actor_id IS NULL)),
        CHECK(revoked_at IS NULL OR length(btrim(coalesce(revoke_reason,'')))>0)
      );
      CREATE UNIQUE INDEX uq_hn_survey_signoff_active ON house_numbering.survey_signoffs
        (organization_id,project_id,source_id,ap,coalesce(ten_duong,'')) WHERE revoked_at IS NULL;
      ALTER TABLE house_numbering.survey_signoffs ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.survey_signoffs FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_survey_signoffs_scope ON house_numbering.survey_signoffs
      FOR ALL TO openvietgis_runtime
      USING (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
      WITH CHECK (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      GRANT SELECT,INSERT,UPDATE ON house_numbering.survey_signoffs TO openvietgis_runtime;

      -- Runs as the owner so the lock holds for every writer (Core editor, import, QGIS sync).
      -- A point is locked when its old OR new hamlet/road is signed (no editing out, moving in or
      -- creating inside). Locks lapse while the plugin is deactivated, so data is never frozen.
      -- The shared advisory lock pairs with the exclusive one taken by sign-off.
      -- SQLSTATE OV409 is rendered by Core as HTTP 409 with this message.
      CREATE FUNCTION house_numbering.guard_signed_survey_point() RETURNS trigger
      LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
      DECLARE r record; s record;
      BEGIN
        FOR r IN SELECT OLD AS f WHERE TG_OP<>'INSERT' UNION ALL SELECT NEW WHERE TG_OP<>'DELETE' LOOP
          CONTINUE WHEN NOT ((r.f).attributes ? 'xu_ly');
          PERFORM pg_advisory_xact_lock_shared(hashtextextended('hn_signoff:'||(r.f).source_id::text,0));
          SELECT x.ap,x.ten_duong INTO s FROM house_numbering.survey_signoffs x
          WHERE x.organization_id=(r.f).organization_id AND x.project_id=(r.f).project_id
            AND x.source_id=(r.f).source_id AND x.revoked_at IS NULL
            AND x.ap=btrim((r.f).attributes->>'ap')
            AND (x.ten_duong IS NULL OR x.ten_duong=btrim((r.f).attributes->>'ten_duong'))
            AND EXISTS (SELECT 1 FROM extensions.project_activations a
              WHERE a.organization_id=x.organization_id AND a.project_id=x.project_id
                AND a.extension_key='house_numbering' AND a.status='active')
          LIMIT 1;
          IF FOUND THEN
            RAISE EXCEPTION USING ERRCODE='OV409',
              MESSAGE='Điểm thuộc '||s.ap||coalesce(' · '||s.ten_duong,'')
                ||' đã được ký duyệt đối chiếu. Người duyệt phải mở khóa trước khi sửa.';
          END IF;
        END LOOP;
        RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
      END $$;
      REVOKE ALL ON FUNCTION house_numbering.guard_signed_survey_point() FROM PUBLIC;
      -- Core bumps content_revision on every row of a source per commit and adds fields with
      -- attributes||{new: null}; neither changes an existing value, so neither is guarded.
      CREATE TRIGGER trg_hn_guard_signed_survey_point
        BEFORE UPDATE OF attributes, geometry ON spatial.reference_features
        FOR EACH ROW WHEN ((OLD.attributes ? 'xu_ly' OR NEW.attributes ? 'xu_ly') AND (
          NOT NEW.attributes @> OLD.attributes
          OR ST_AsEWKB(OLD.geometry) IS DISTINCT FROM ST_AsEWKB(NEW.geometry)))
        EXECUTE FUNCTION house_numbering.guard_signed_survey_point();
      CREATE TRIGGER trg_hn_guard_signed_survey_point_insert
        BEFORE INSERT ON spatial.reference_features
        FOR EACH ROW WHEN (NEW.attributes ? 'xu_ly')
        EXECUTE FUNCTION house_numbering.guard_signed_survey_point();
      CREATE TRIGGER trg_hn_guard_signed_survey_point_delete
        BEFORE DELETE ON spatial.reference_features
        FOR EACH ROW WHEN (OLD.attributes ? 'xu_ly')
        EXECUTE FUNCTION house_numbering.guard_signed_survey_point();
    """)


def downgrade():
    op.execute("""
      DROP TRIGGER trg_hn_guard_signed_survey_point_delete ON spatial.reference_features;
      DROP TRIGGER trg_hn_guard_signed_survey_point_insert ON spatial.reference_features;
      DROP TRIGGER trg_hn_guard_signed_survey_point ON spatial.reference_features;
      DROP FUNCTION house_numbering.guard_signed_survey_point();
      DROP TABLE house_numbering.survey_signoffs;
    """)
