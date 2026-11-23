"""Allow a route to reference several Core features without duplicating geometry."""

from alembic import op

revision = "hn_0006_route_segments"
down_revision = "hn_0005_building_lineage_plaque"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("""
        ALTER TABLE house_numbering.routes ADD COLUMN segments jsonb NOT NULL DEFAULT '[]'::jsonb,
        ADD CONSTRAINT ck_hn_route_segments CHECK (jsonb_typeof(segments)='array' AND jsonb_array_length(segments)<=200)
    """)
    op.execute("""
        CREATE FUNCTION house_numbering.resolve_route_line(parts geometry[])
        RETURNS geometry LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$
          WITH checked AS (
            SELECT count(*) AS n, count(DISTINCT ST_SRID(p)) AS crs_count,
              bool_and(p IS NOT NULL AND ST_SRID(p)>0 AND ST_GeometryType(p) IN ('ST_LineString','ST_MultiLineString')
                AND NOT ST_IsEmpty(p)) AS usable
            FROM unnest(parts) p
          ), merged AS (
            SELECT CASE WHEN n>0 AND crs_count=1 AND usable THEN
              CASE WHEN n=1 AND ST_GeometryType(parts[1])='ST_LineString' THEN parts[1]
              ELSE ST_LineMerge(ST_CollectionExtract(ST_Collect(parts),2)) END
            END AS g,n FROM checked
          ), valid AS (
            SELECT g,n FROM merged WHERE ST_GeometryType(g)='ST_LineString' AND ST_Length(g)>0
              AND (n=1 AND ST_GeometryType(parts[1])='ST_LineString' OR ST_IsSimple(g) AND NOT ST_IsClosed(g))
          )
          SELECT CASE WHEN n=1 AND ST_GeometryType(parts[1])='ST_LineString' THEN g
            WHEN ST_LineLocatePoint(g,ST_StartPoint(ST_GeometryN(parts[1],1))) >
              ST_LineLocatePoint(g,ST_EndPoint(ST_GeometryN(parts[1],1))) THEN ST_Reverse(g)
            ELSE g END FROM valid
        $$
    """)
    # An invoker view preserves underlying Core/module RLS and avoids storing copied geometry.
    op.execute("""
        CREATE VIEW house_numbering.route_lines WITH (security_invoker=true) AS
        SELECT r.organization_id,r.project_id,r.id AS route_id,g.geometry,g.content_revision
        FROM house_numbering.routes r
        CROSS JOIN LATERAL (
            SELECT house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) AS geometry,
              CASE WHEN count(*)=1 THEN min(f.content_revision)
                ELSE encode(sha256(convert_to(string_agg((l.v->>'source_id')||':'||(l.v->>'feature_id')||':'||f.content_revision,'|' ORDER BY l.ord),'UTF8')),'hex')
              END AS content_revision
            FROM jsonb_array_elements(CASE WHEN jsonb_array_length(r.segments)>0 THEN r.segments
                 ELSE jsonb_build_array(jsonb_build_object('source_id',r.linked_source_id,'feature_id',r.linked_feature_id)) END)
              WITH ORDINALITY l(v,ord)
            LEFT JOIN spatial.reference_features f ON f.organization_id=r.organization_id AND f.project_id=r.project_id
              AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
            LEFT JOIN spatial.data_sources s ON s.organization_id=r.organization_id AND s.project_id=r.project_id AND s.id=f.source_id
            HAVING bool_and(COALESCE(f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready',false))
        ) g WHERE g.geometry IS NOT NULL
    """)
    op.execute("GRANT SELECT ON house_numbering.route_lines TO openvietgis_runtime")


def downgrade() -> None:
    # Refuse lossy downgrade while grouped routes exist.
    op.execute("""
      DO $$ BEGIN
        IF EXISTS (SELECT 1 FROM house_numbering.routes WHERE jsonb_array_length(segments)>0) THEN
          RAISE EXCEPTION 'Cannot downgrade while routes have segment links';
        END IF;
      END $$
    """)
    op.execute("DROP VIEW house_numbering.route_lines")
    op.execute("DROP FUNCTION house_numbering.resolve_route_line(geometry[])")
    op.execute("ALTER TABLE house_numbering.routes DROP COLUMN segments")
