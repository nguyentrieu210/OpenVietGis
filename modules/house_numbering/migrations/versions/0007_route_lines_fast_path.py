"""Keep existing single-feature routes on the indexed Core join."""
from alembic import op

revision = "hn_0007_route_lines_fast_path"
down_revision = "hn_0006_route_segments"
branch_labels = None
depends_on = None

_GROUPED = """
        SELECT r.organization_id,r.project_id,r.id AS route_id,g.geometry,g.content_revision
        FROM house_numbering.routes r
        CROSS JOIN LATERAL (
            SELECT house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) AS geometry,
              CASE WHEN count(*)=1 THEN min(f.content_revision)
                ELSE encode(sha256(convert_to(string_agg((l.v->>'source_id')||':'||(l.v->>'feature_id')||':'||f.content_revision,'|' ORDER BY l.ord),'UTF8')),'hex')
              END AS content_revision
            FROM jsonb_array_elements({links}) WITH ORDINALITY l(v,ord)
            LEFT JOIN spatial.reference_features f ON f.organization_id=r.organization_id AND f.project_id=r.project_id
              AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
            LEFT JOIN spatial.data_sources s ON s.organization_id=r.organization_id AND s.project_id=r.project_id AND s.id=f.source_id
            HAVING bool_and(COALESCE(f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready',false))
        ) g WHERE g.geometry IS NOT NULL {where}
"""
_SINGLE = """
        FROM house_numbering.routes r JOIN spatial.reference_features f
          ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(r.organization_id,r.project_id,r.linked_source_id,r.linked_feature_id)
        JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
        WHERE jsonb_array_length(r.segments)=0 AND f.geometry_state='present' AND ST_SRID(f.geometry)>0
          AND s.lifecycle='active' AND s.validation_state='ready' 
"""

def upgrade() -> None:
    op.execute("""
      CREATE OR REPLACE VIEW house_numbering.route_lines WITH (security_invoker=true) AS
      SELECT r.organization_id,r.project_id,r.id AS route_id,f.geometry AS geometry,f.content_revision::text AS content_revision
    """ + _SINGLE + """
      AND ST_GeometryType(f.geometry)='ST_LineString' AND ST_Length(f.geometry)>0
      UNION ALL
      SELECT r.organization_id,r.project_id,r.id AS route_id,house_numbering.resolve_route_line(ARRAY[f.geometry]) AS geometry,f.content_revision::text AS content_revision
    """ + _SINGLE + """
      AND ST_GeometryType(f.geometry)='ST_MultiLineString' AND house_numbering.resolve_route_line(ARRAY[f.geometry]) IS NOT NULL
      UNION ALL 
    """ + _GROUPED.format(links="r.segments",where="AND jsonb_array_length(r.segments)>0"))

def downgrade() -> None:
    op.execute("CREATE OR REPLACE VIEW house_numbering.route_lines WITH (security_invoker=true) AS " +
        _GROUPED.format(links="CASE WHEN jsonb_array_length(r.segments)>0 THEN r.segments ELSE jsonb_build_array(jsonb_build_object('source_id',r.linked_source_id,'feature_id',r.linked_feature_id)) END",where=""))
