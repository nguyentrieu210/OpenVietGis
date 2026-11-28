"""Preserve RLS denials for out-of-scope relation writes."""
from alembic import op

revision = "0052_relation_write_scope"
down_revision = "0051_native_vn2000_export"
branch_labels = None
depends_on = None


def upgrade():
    # BEFORE triggers run before RLS. For a runtime write whose row scope does
    # not match the request GUCs, let table RLS reject it with its established
    # 42501 SQLSTATE; do not replace that denial with a relation-lock OV409.
    # Exact-scope relation-bound writes still take the project lock and fail
    # closed with OV409 when the actor has only restricted project access.
    op.execute("""
        CREATE OR REPLACE FUNCTION editing.lock_relation_write() RETURNS trigger
        LANGUAGE plpgsql SET search_path=pg_catalog AS $$
        DECLARE org uuid; project uuid; source uuid;
                request_org uuid; request_project uuid;
        BEGIN
          IF TG_OP='UPDATE' THEN
            IF NEW.attributes IS NOT DISTINCT FROM OLD.attributes THEN RETURN NEW; END IF;
          END IF;
          IF TG_OP='DELETE' THEN
            org:=OLD.organization_id; project:=OLD.project_id; source:=OLD.source_id;
          ELSE
            org:=NEW.organization_id; project:=NEW.project_id; source:=NEW.source_id;
          END IF;
          IF current_user='openvietgis_runtime' THEN
            request_org:=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid;
            request_project:=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid;
            IF org IS DISTINCT FROM request_org OR project IS DISTINCT FROM request_project THEN
              IF TG_OP='DELETE' THEN RETURN OLD; END IF;
              RETURN NEW;
            END IF;
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
              MESSAGE='Relation-bound writes require unrestricted project access';
          END IF;
          IF TG_OP='DELETE' THEN RETURN OLD; END IF;
          RETURN NEW;
        END $$
    """)


def downgrade():
    # Restore the exact pre-0052 behavior from migration 0049.
    op.execute("""
        CREATE OR REPLACE FUNCTION editing.lock_relation_write() RETURNS trigger
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
              MESSAGE='Relation-bound writes require unrestricted project access';
          END IF;
          IF TG_OP='DELETE' THEN RETURN OLD; END IF;
          RETURN NEW;
        END $$
    """)
