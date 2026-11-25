"""Keep current Số nhà links valid when Core or QGIS deletes a feature."""

from alembic import op

revision = "hn_0012_linked_feature_guard"
down_revision = "hn_0011_survey_signoffs"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("""
        CREATE INDEX ix_hn_areas_feature_link
          ON house_numbering.areas (organization_id,project_id,linked_source_id,linked_feature_id);
        CREATE INDEX ix_hn_routes_feature_link
          ON house_numbering.routes (organization_id,project_id,linked_source_id,linked_feature_id);
        CREATE INDEX ix_hn_buildings_feature_link
          ON house_numbering.buildings (organization_id,project_id,linked_source_id,linked_feature_id)
          WHERE state='active';
        CREATE INDEX ix_hn_route_segment_link
          ON house_numbering.routes USING gin (segments jsonb_path_ops)
          WHERE segments <> '[]'::jsonb;

        CREATE FUNCTION house_numbering.pin_linked_feature() RETURNS trigger
        LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
        DECLARE refs jsonb; ref record;
        BEGIN
          -- Acquire the same row locks as a foreign key would, without locking
          -- historical references on retired buildings or changing legacy rows.
          IF TG_TABLE_NAME='buildings' THEN
            IF NEW.state <> 'active' THEN RETURN NEW; END IF;
          END IF;
          refs := jsonb_build_array(jsonb_build_object(
            'source_id', NEW.linked_source_id, 'feature_id', NEW.linked_feature_id
          ));
          IF TG_TABLE_NAME='routes' THEN
            refs := refs || NEW.segments;
          END IF;
          FOR ref IN
            SELECT DISTINCT (link->>'source_id')::uuid AS source_id,
                            link->>'feature_id' AS feature_id
            FROM jsonb_array_elements(refs) AS l(link)
            WHERE link->>'source_id' IS NOT NULL AND link->>'feature_id' IS NOT NULL
            ORDER BY 1,2
          LOOP
            PERFORM 1 FROM spatial.reference_features f
              WHERE f.organization_id=NEW.organization_id AND f.project_id=NEW.project_id
                AND f.source_id=ref.source_id AND f.feature_id=ref.feature_id
              FOR KEY SHARE;
            IF NOT FOUND THEN
              RAISE EXCEPTION USING ERRCODE='OV409',
                MESSAGE='Đối tượng GIS được liên kết đã thay đổi hoặc không còn tồn tại.';
            END IF;
          END LOOP;
          RETURN NEW;
        END $$;
        REVOKE ALL ON FUNCTION house_numbering.pin_linked_feature() FROM PUBLIC;

        CREATE TRIGGER trg_hn_pin_area_link
          BEFORE INSERT OR UPDATE OF linked_source_id,linked_feature_id ON house_numbering.areas
          FOR EACH ROW EXECUTE FUNCTION house_numbering.pin_linked_feature();
        CREATE TRIGGER trg_hn_pin_route_link
          BEFORE INSERT OR UPDATE OF linked_source_id,linked_feature_id,segments
          ON house_numbering.routes FOR EACH ROW EXECUTE FUNCTION house_numbering.pin_linked_feature();
        CREATE TRIGGER trg_hn_pin_building_link
          BEFORE INSERT OR UPDATE OF linked_source_id,linked_feature_id,state
          ON house_numbering.buildings
          FOR EACH ROW EXECUTE FUNCTION house_numbering.pin_linked_feature();

        CREATE FUNCTION house_numbering.guard_linked_feature_delete() RETURNS trigger
        LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
        BEGIN
          IF EXISTS (
              SELECT 1 FROM house_numbering.buildings b
              WHERE (b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
                  = (OLD.organization_id,OLD.project_id,OLD.source_id,OLD.feature_id)
                AND b.state='active'
            ) OR EXISTS (
              SELECT 1 FROM house_numbering.areas a
              WHERE (a.organization_id,a.project_id,a.linked_source_id,a.linked_feature_id)
                  = (OLD.organization_id,OLD.project_id,OLD.source_id,OLD.feature_id)
            ) OR EXISTS (
              SELECT 1 FROM house_numbering.routes r
              WHERE (r.organization_id,r.project_id) = (OLD.organization_id,OLD.project_id)
                AND (
                  (r.linked_source_id,r.linked_feature_id) = (OLD.source_id,OLD.feature_id)
                  OR (r.segments <> '[]'::jsonb AND r.segments @> jsonb_build_array(
                      jsonb_build_object('source_id',OLD.source_id,'feature_id',OLD.feature_id)
                    ))
                )
            ) THEN
              RAISE EXCEPTION USING ERRCODE='OV409',
                MESSAGE='Đối tượng GIS còn liên kết với hồ sơ số nhà. Xử lý liên kết trước khi xóa.';
          END IF;
          RETURN OLD;
        END $$;
        REVOKE ALL ON FUNCTION house_numbering.guard_linked_feature_delete() FROM PUBLIC;
        CREATE TRIGGER trg_hn_guard_linked_feature_delete
          BEFORE DELETE ON spatial.reference_features
          FOR EACH ROW EXECUTE FUNCTION house_numbering.guard_linked_feature_delete();
    """)


def downgrade() -> None:
    op.execute("""
        DROP TRIGGER trg_hn_guard_linked_feature_delete ON spatial.reference_features;
        DROP TRIGGER trg_hn_pin_building_link ON house_numbering.buildings;
        DROP TRIGGER trg_hn_pin_route_link ON house_numbering.routes;
        DROP TRIGGER trg_hn_pin_area_link ON house_numbering.areas;
        DROP FUNCTION house_numbering.guard_linked_feature_delete();
        DROP FUNCTION house_numbering.pin_linked_feature();
        DROP INDEX house_numbering.ix_hn_route_segment_link;
        DROP INDEX house_numbering.ix_hn_buildings_feature_link;
        DROP INDEX house_numbering.ix_hn_routes_feature_link;
        DROP INDEX house_numbering.ix_hn_areas_feature_link;
    """)
