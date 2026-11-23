"""Human publisher profiles and immutable marketplace submission/review queue."""

from alembic import op

revision = "0028_marketplace_developers"
down_revision = "0027_partner_platform"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE extensions.marketplace_publishers (
        id varchar(32) PRIMARY KEY CHECK(id ~ '^dev_[a-f0-9]{24}$'),
        owner_user_id uuid NOT NULL UNIQUE REFERENCES identity.users(id) ON DELETE RESTRICT,
        name varchar(120) NOT NULL CHECK(length(trim(name))>0),
        public_key varchar(44) NOT NULL CHECK(length(public_key)=44),
        created_at timestamptz NOT NULL DEFAULT now(),
        UNIQUE(id,owner_user_id)
      );
      CREATE TABLE extensions.marketplace_submissions (
        id uuid PRIMARY KEY, publisher_id varchar(32) NOT NULL,
        owner_user_id uuid NOT NULL,
        extension_key varchar(100) NOT NULL, version varchar(32) NOT NULL,
        manifest jsonb NOT NULL CHECK(octet_length(manifest::text)<=32768),
        signature varchar(88) NOT NULL CHECK(length(signature)=88),
        status varchar(16) NOT NULL DEFAULT 'pending'
          CHECK(status IN ('pending','approved','rejected','withdrawn')),
        review_note varchar(2000), reviewed_by uuid REFERENCES identity.users(id),
        reviewed_at timestamptz, created_at timestamptz NOT NULL DEFAULT now(),
        FOREIGN KEY(publisher_id,owner_user_id)
          REFERENCES extensions.marketplace_publishers(id,owner_user_id),
        UNIQUE(extension_key,version),
        CHECK((status IN ('approved','rejected') AND reviewed_by IS NOT NULL
          AND reviewed_at IS NOT NULL AND review_note IS NOT NULL AND length(trim(review_note))>0
          AND reviewed_by<>owner_user_id)
          OR (status IN ('pending','withdrawn') AND reviewed_by IS NULL
          AND reviewed_at IS NULL AND review_note IS NULL))
      );
      CREATE INDEX marketplace_review_queue ON extensions.marketplace_submissions
        (status,created_at);
      CREATE INDEX marketplace_owner_queue ON extensions.marketplace_submissions(owner_user_id);
      ALTER TABLE extensions.marketplace_publishers ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_publishers FORCE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_submissions ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_submissions FORCE ROW LEVEL SECURITY;
      CREATE POLICY marketplace_publisher_read ON extensions.marketplace_publishers
        FOR SELECT TO openvietgis_runtime USING(
          owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
          OR current_setting('openvietgis.marketplace_reviewer',true)='yes');
      CREATE POLICY marketplace_publisher_create ON extensions.marketplace_publishers
        FOR INSERT TO openvietgis_runtime WITH CHECK(
          owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid);
      CREATE POLICY marketplace_release_read ON extensions.marketplace_submissions
        FOR SELECT TO openvietgis_runtime USING(
          owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
          OR current_setting('openvietgis.marketplace_reviewer',true)='yes');
      CREATE POLICY marketplace_release_create ON extensions.marketplace_submissions
        FOR INSERT TO openvietgis_runtime WITH CHECK(
          owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
          AND status='pending');
      CREATE POLICY marketplace_release_withdraw ON extensions.marketplace_submissions
        FOR UPDATE TO openvietgis_runtime USING(
          owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
          AND status='pending') WITH CHECK(
          owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
          AND status='withdrawn');
      CREATE POLICY marketplace_release_review ON extensions.marketplace_submissions
        FOR UPDATE TO openvietgis_runtime USING(
          current_setting('openvietgis.marketplace_reviewer',true)='yes'
          AND status='pending'
          AND owner_user_id<>NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid)
        WITH CHECK(current_setting('openvietgis.marketplace_reviewer',true)='yes'
          AND status IN ('approved','rejected')
          AND reviewed_by=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
          AND owner_user_id<>reviewed_by);
      GRANT SELECT,INSERT ON extensions.marketplace_publishers TO openvietgis_runtime;
      GRANT SELECT,INSERT ON extensions.marketplace_submissions TO openvietgis_runtime;
      GRANT UPDATE(status,review_note,reviewed_by,reviewed_at)
        ON extensions.marketplace_submissions TO openvietgis_runtime;
    """)


def downgrade():
    op.execute("DROP TABLE extensions.marketplace_submissions")
    op.execute("DROP TABLE extensions.marketplace_publishers")
