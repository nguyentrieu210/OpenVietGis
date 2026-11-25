"""Align database map validation with optional layer group/display (Spec 056)."""
from alembic import op

revision = "0046_map_display_config"
down_revision = "0045_postgis_bbox_leakproof"
branch_labels = None
depends_on = None

_BASE = """
        CREATE OR REPLACE FUNCTION maps.valid_configuration(v jsonb) RETURNS boolean
        LANGUAGE plpgsql IMMUTABLE SET search_path=pg_catalog AS $$
        DECLARE item jsonb; s jsonb; p jsonb; ids uuid[] := ARRAY[]::uuid[]; ident uuid;
        BEGIN
            IF v IS NULL OR jsonb_typeof(v)<>'object'
                OR NOT v ?& ARRAY['layers','basemap','default_viewpoint']
                OR v-ARRAY['layers','basemap','default_viewpoint']<>'{}'::jsonb
                OR v->'basemap' IS DISTINCT FROM '\"grid\"'::jsonb
                OR jsonb_typeof(v->'layers')<>'array'
            THEN RETURN false; END IF;
            IF jsonb_array_length(v->'layers')>32 THEN RETURN false; END IF;
            FOR item IN SELECT value FROM jsonb_array_elements(v->'layers') LOOP
                IF jsonb_typeof(item)<>'object'
                    OR NOT item ?& ARRAY['layer_id','visible','style']
                    OR item-ARRAY['layer_id','visible','style']<>'{}'::jsonb
                    OR jsonb_typeof(item->'layer_id')<>'string'
                    OR jsonb_typeof(item->'visible')<>'boolean'
                THEN RETURN false; END IF;
                ident := (item->>'layer_id')::uuid;
                IF ident=ANY(ids) THEN RETURN false; END IF;
                ids := array_append(ids,ident);
                s:=item->'style';
                IF jsonb_typeof(s)<>'object'
                    OR NOT s ?& ARRAY['color','opacity','width','radius']
                    OR s-ARRAY['color','opacity','width','radius']<>'{}'::jsonb
                    OR jsonb_typeof(s->'color')<>'string'
                    OR (s->>'color') !~ '^#[0-9A-F]{6}$'
                    OR NOT maps.valid_number(s->'opacity',0,1)
                    OR NOT maps.valid_number(s->'width',1,12)
                    OR NOT maps.valid_number(s->'radius',2,20)
                THEN RETURN false; END IF;
            END LOOP;
            p:=v->'default_viewpoint';
            IF p='null'::jsonb THEN RETURN true; END IF;
            RETURN COALESCE(
                jsonb_typeof(p)='object'
                AND p ?& ARRAY['longitude','latitude','zoom','bearing','pitch','crs']
                AND p-ARRAY['longitude','latitude','zoom','bearing','pitch','crs']='{}'::jsonb
                AND maps.valid_number(p->'longitude',-180,180)
                AND maps.valid_number(p->'latitude',-85.05112878,85.05112878)
                AND maps.valid_number(p->'zoom',0,22)
                AND maps.valid_number(p->'bearing',-180,180)
                AND maps.valid_number(p->'pitch',0,60)
                AND p->'crs'='4326'::jsonb,false);
        EXCEPTION WHEN invalid_text_representation OR numeric_value_out_of_range
            THEN RETURN false;
        END $$
"""
_EXTRA = """
                IF item ? 'group' THEN
                    IF jsonb_typeof(item->'group') <> 'string'
                        OR char_length(item->>'group') NOT BETWEEN 1 AND 200
                        OR cardinality(string_to_array(item->>'group','/')) > 4
                        OR EXISTS (SELECT 1 FROM unnest(string_to_array(item->>'group','/')) AS parts(part)
                            WHERE part = '' OR part ~ '^[[:space:]]|[[:space:]]$')
                    THEN RETURN false; END IF;
                END IF;
                IF item ? 'display' THEN
                    IF jsonb_typeof(item->'display') <> 'object'
                        OR octet_length((item->'display')::text) > 16000
                    THEN RETURN false; END IF;
                END IF;
"""


def upgrade():
    op.execute(_BASE.replace(
        "item-ARRAY['layer_id','visible','style']",
        "item-ARRAY['layer_id','visible','style','group','display']",
    ).replace("                ident :=", _EXTRA + "                ident :="))


def downgrade():
    # Never silently discard extended layer data or leave invalid historical rows.
    # Revalidating explicit constraints checks every row regardless of runtime RLS.
    op.execute(_BASE)
    op.execute("ALTER TABLE maps.project_maps DROP CONSTRAINT ck_map_configuration")
    op.execute("ALTER TABLE maps.project_maps ADD CONSTRAINT ck_map_configuration CHECK(maps.valid_configuration(configuration))")
    op.execute("ALTER TABLE maps.map_revisions DROP CONSTRAINT ck_map_revision_configurations")
    op.execute("""ALTER TABLE maps.map_revisions ADD CONSTRAINT ck_map_revision_configurations CHECK(
        maps.valid_configuration(after_configuration)
        AND (before_configuration IS NULL OR maps.valid_configuration(before_configuration))
        AND (before_configuration IS NULL OR before_configuration<>after_configuration))""")
