"""Publisher key generations and immutable historical signing key snapshots."""
from alembic import op
revision = "0060_plugin_publisher_rotation"
down_revision = "0059_plugin_global_trust"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      ALTER TABLE extensions.marketplace_publishers ADD COLUMN key_revision bigint NOT NULL DEFAULT 1 CHECK(key_revision>0);
      ALTER TABLE extensions.marketplace_submissions ADD COLUMN signing_public_key varchar(44) CHECK(length(signing_public_key)=44);
      ALTER TABLE extensions.marketplace_publishers NO FORCE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_submissions NO FORCE ROW LEVEL SECURITY;
      UPDATE extensions.marketplace_submissions s SET signing_public_key=p.public_key
        FROM extensions.marketplace_publishers p WHERE p.id=s.publisher_id;
      ALTER TABLE extensions.marketplace_submissions ALTER COLUMN signing_public_key SET NOT NULL;
      ALTER TABLE extensions.marketplace_publishers FORCE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_submissions FORCE ROW LEVEL SECURITY;
      CREATE FUNCTION extensions.snapshot_marketplace_signing_key() RETURNS trigger
        LANGUAGE plpgsql SET search_path=pg_catalog,extensions AS $$
        BEGIN
          SELECT public_key INTO NEW.signing_public_key FROM extensions.marketplace_publishers
            WHERE id=NEW.publisher_id AND owner_user_id=NEW.owner_user_id;
          IF NEW.signing_public_key IS NULL THEN RAISE EXCEPTION 'publisher signing key unavailable'; END IF;
          RETURN NEW;
        END $$;
      CREATE TRIGGER snapshot_marketplace_signing_key BEFORE INSERT ON extensions.marketplace_submissions
        FOR EACH ROW EXECUTE FUNCTION extensions.snapshot_marketplace_signing_key();
      CREATE TABLE extensions.marketplace_publisher_rotations (
        publisher_id varchar(32) NOT NULL,owner_user_id uuid NOT NULL,
        revision bigint NOT NULL CHECK(revision>1),old_public_key varchar(44) NOT NULL CHECK(length(old_public_key)=44),
        new_public_key varchar(44) NOT NULL CHECK(length(new_public_key)=44 AND new_public_key<>old_public_key),
        old_proof varchar(88) NOT NULL CHECK(length(old_proof)=88),new_proof varchar(88) NOT NULL CHECK(length(new_proof)=88),
        created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
        PRIMARY KEY(publisher_id,revision), UNIQUE(publisher_id,new_public_key),
        FOREIGN KEY(publisher_id,owner_user_id) REFERENCES extensions.marketplace_publishers(id,owner_user_id)
      );
      ALTER TABLE extensions.marketplace_publisher_rotations ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_publisher_rotations FORCE ROW LEVEL SECURITY;
      CREATE POLICY publisher_rotation_read ON extensions.marketplace_publisher_rotations FOR SELECT TO openvietgis_runtime USING(
        owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid OR current_setting('openvietgis.marketplace_reviewer',true)='yes');
      CREATE POLICY publisher_rotation_insert ON extensions.marketplace_publisher_rotations FOR INSERT TO openvietgis_runtime WITH CHECK(
        owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid);
      CREATE POLICY publisher_key_update ON extensions.marketplace_publishers FOR UPDATE TO openvietgis_runtime USING(
        owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid)
        WITH CHECK(owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid);
      GRANT UPDATE(public_key,key_revision) ON extensions.marketplace_publishers TO openvietgis_runtime;
      GRANT SELECT,INSERT ON extensions.marketplace_publisher_rotations TO openvietgis_runtime;
    """)

def downgrade():
    op.execute("ALTER TABLE extensions.marketplace_publisher_rotations NO FORCE ROW LEVEL SECURITY")
    if op.get_bind().exec_driver_sql("SELECT EXISTS(SELECT 1 FROM extensions.marketplace_publisher_rotations)").scalar():
        raise RuntimeError("Preserve historical signing keys before downgrade")
    op.execute("""
      DROP TABLE extensions.marketplace_publisher_rotations;
      REVOKE UPDATE(public_key,key_revision) ON extensions.marketplace_publishers FROM openvietgis_runtime;
      DROP POLICY publisher_key_update ON extensions.marketplace_publishers;
      DROP TRIGGER snapshot_marketplace_signing_key ON extensions.marketplace_submissions;
      DROP FUNCTION extensions.snapshot_marketplace_signing_key();
      ALTER TABLE extensions.marketplace_submissions DROP COLUMN signing_public_key;
      ALTER TABLE extensions.marketplace_publishers DROP COLUMN key_revision;
    """)
