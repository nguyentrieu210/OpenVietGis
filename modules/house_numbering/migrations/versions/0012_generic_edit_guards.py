"""Translate survey signoffs into Core guard policies, retaining source locks and existing signoffs."""
from alembic import op

revision = "hn_0012_generic_edit_guards"
down_revision = "hn_0011_survey_signoffs"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      -- Core migration must run first. No loss of signoffs; old locks stay in force until this transaction commits.
      LOCK TABLE spatial.reference_features IN SHARE ROW EXCLUSIVE MODE;
      LOCK TABLE house_numbering.survey_signoffs IN SHARE ROW EXCLUSIVE MODE;
      INSERT INTO extensions.feature_guard_providers(extension_key,lock_namespace,required_fields)
        VALUES ('house_numbering','hn_signoff:',ARRAY['xu_ly']);
      INSERT INTO extensions.feature_guard_locks(extension_key,lock_id,organization_id,project_id,source_id,scope_values,message)
        SELECT 'house_numbering',id,organization_id,project_id,source_id,
          jsonb_strip_nulls(jsonb_build_object('ap',ap,'ten_duong',ten_duong)),
          'Điểm thuộc '||ap||coalesce(' · '||ten_duong,'')||' đã được ký duyệt đối chiếu. Người duyệt phải mở khóa trước khi sửa.'
        FROM house_numbering.survey_signoffs WHERE revoked_at IS NULL;
      CREATE FUNCTION house_numbering.sync_core_feature_guard() RETURNS trigger
      LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
      BEGIN
        IF TG_OP<>'INSERT' THEN DELETE FROM extensions.feature_guard_locks
          WHERE extension_key='house_numbering' AND lock_id=OLD.id; END IF;
        IF TG_OP<>'DELETE' AND NEW.revoked_at IS NULL THEN
          INSERT INTO extensions.feature_guard_locks(extension_key,lock_id,organization_id,project_id,source_id,scope_values,message)
          VALUES ('house_numbering',NEW.id,NEW.organization_id,NEW.project_id,NEW.source_id,
            jsonb_strip_nulls(jsonb_build_object('ap',NEW.ap,'ten_duong',NEW.ten_duong)),
            'Điểm thuộc '||NEW.ap||coalesce(' · '||NEW.ten_duong,'')||' đã được ký duyệt đối chiếu. Người duyệt phải mở khóa trước khi sửa.');
        END IF;
        RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
      END $$;
      REVOKE ALL ON FUNCTION house_numbering.sync_core_feature_guard() FROM PUBLIC;
      CREATE TRIGGER trg_hn_sync_core_feature_guard AFTER INSERT OR UPDATE OR DELETE ON house_numbering.survey_signoffs
        FOR EACH ROW EXECUTE FUNCTION house_numbering.sync_core_feature_guard();
      DROP TRIGGER trg_hn_guard_signed_survey_point_delete ON spatial.reference_features;
      DROP TRIGGER trg_hn_guard_signed_survey_point_insert ON spatial.reference_features;
      DROP TRIGGER trg_hn_guard_signed_survey_point ON spatial.reference_features;
    """)


def downgrade():
    op.execute("""
      LOCK TABLE spatial.reference_features IN SHARE ROW EXCLUSIVE MODE;
      LOCK TABLE house_numbering.survey_signoffs IN SHARE ROW EXCLUSIVE MODE;
      CREATE TRIGGER trg_hn_guard_signed_survey_point BEFORE UPDATE OF attributes,geometry ON spatial.reference_features
        FOR EACH ROW WHEN ((OLD.attributes ? 'xu_ly' OR NEW.attributes ? 'xu_ly') AND
          (NOT NEW.attributes @> OLD.attributes OR ST_AsEWKB(OLD.geometry) IS DISTINCT FROM ST_AsEWKB(NEW.geometry)))
        EXECUTE FUNCTION house_numbering.guard_signed_survey_point();
      CREATE TRIGGER trg_hn_guard_signed_survey_point_insert BEFORE INSERT ON spatial.reference_features
        FOR EACH ROW WHEN (NEW.attributes ? 'xu_ly') EXECUTE FUNCTION house_numbering.guard_signed_survey_point();
      CREATE TRIGGER trg_hn_guard_signed_survey_point_delete BEFORE DELETE ON spatial.reference_features
        FOR EACH ROW WHEN (OLD.attributes ? 'xu_ly') EXECUTE FUNCTION house_numbering.guard_signed_survey_point();
      DROP TRIGGER trg_hn_sync_core_feature_guard ON house_numbering.survey_signoffs;
      DROP FUNCTION house_numbering.sync_core_feature_guard();
      DELETE FROM extensions.feature_guard_locks WHERE extension_key='house_numbering';
      DELETE FROM extensions.feature_guard_providers WHERE extension_key='house_numbering';
    """)
