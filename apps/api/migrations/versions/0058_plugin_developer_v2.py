"""Add immutable signed declarative payloads to existing publisher submissions."""
from alembic import op

revision = "0058_plugin_developer_v2"
down_revision = "0057_plugin_processing_proposals"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE extensions.marketplace_v2_submissions (
        submission_id uuid PRIMARY KEY REFERENCES extensions.marketplace_submissions(id) ON DELETE RESTRICT,
        owner_user_id uuid NOT NULL REFERENCES identity.users(id) ON DELETE RESTRICT,
        bundle jsonb NOT NULL CHECK(jsonb_typeof(bundle)='object' AND octet_length(bundle::text)<=524288),
        release_digest varchar(64) NOT NULL CHECK(release_digest ~ '^[a-f0-9]{64}$'),
        status varchar(16) NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','withdrawn')),
        review_note varchar(2000), reviewed_by uuid REFERENCES identity.users(id), reviewed_at timestamptz,
        created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
        CHECK((status IN ('approved','rejected') AND reviewed_by IS NOT NULL AND reviewed_by<>owner_user_id
          AND reviewed_at IS NOT NULL AND review_note IS NOT NULL AND length(trim(review_note))>0)
          OR (status IN ('pending','withdrawn') AND reviewed_by IS NULL AND reviewed_at IS NULL AND review_note IS NULL))
      );
      CREATE TABLE extensions.marketplace_v2_withdrawals (
        submission_id uuid PRIMARY KEY REFERENCES extensions.marketplace_v2_submissions(submission_id) ON DELETE RESTRICT,
        requested_by uuid NOT NULL REFERENCES identity.users(id) ON DELETE RESTRICT,
        reason varchar(2000) NOT NULL CHECK(length(trim(reason))>0),
        created_at timestamptz NOT NULL DEFAULT clock_timestamp()
      );
      CREATE INDEX marketplace_v2_review_queue ON extensions.marketplace_v2_submissions(status,created_at,submission_id);
      CREATE INDEX marketplace_v2_owner_queue ON extensions.marketplace_v2_submissions(owner_user_id);
      ALTER TABLE extensions.marketplace_v2_submissions ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_v2_submissions FORCE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_v2_withdrawals ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.marketplace_v2_withdrawals FORCE ROW LEVEL SECURITY;
      CREATE POLICY v2_submission_read ON extensions.marketplace_v2_submissions FOR SELECT TO openvietgis_runtime USING(
        owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
        OR current_setting('openvietgis.marketplace_reviewer',true)='yes');
      CREATE POLICY v2_submission_insert ON extensions.marketplace_v2_submissions FOR INSERT TO openvietgis_runtime WITH CHECK(
        owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid AND status='pending'
        AND EXISTS(SELECT 1 FROM extensions.marketplace_submissions s WHERE s.id=submission_id
          AND s.owner_user_id=marketplace_v2_submissions.owner_user_id AND s.status IN ('pending','approved')));
      CREATE POLICY v2_submission_withdraw ON extensions.marketplace_v2_submissions FOR UPDATE TO openvietgis_runtime
        USING(owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid AND status='pending')
        WITH CHECK(owner_user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid AND status='withdrawn');
      CREATE POLICY v2_submission_review ON extensions.marketplace_v2_submissions FOR UPDATE TO openvietgis_runtime
        USING(current_setting('openvietgis.marketplace_reviewer',true)='yes' AND status='pending'
          AND owner_user_id<>NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid)
        WITH CHECK(current_setting('openvietgis.marketplace_reviewer',true)='yes' AND status IN ('approved','rejected')
          AND reviewed_by=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid AND owner_user_id<>reviewed_by);
      CREATE POLICY v2_withdrawal_read ON extensions.marketplace_v2_withdrawals FOR SELECT TO openvietgis_runtime USING(
        EXISTS(SELECT 1 FROM extensions.marketplace_v2_submissions s WHERE s.submission_id=marketplace_v2_withdrawals.submission_id));
      CREATE POLICY v2_withdrawal_insert ON extensions.marketplace_v2_withdrawals FOR INSERT TO openvietgis_runtime WITH CHECK(
        requested_by=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
        AND EXISTS(SELECT 1 FROM extensions.marketplace_v2_submissions s WHERE s.submission_id=marketplace_v2_withdrawals.submission_id
        AND s.status='approved'));
      GRANT SELECT,INSERT ON extensions.marketplace_v2_submissions,extensions.marketplace_v2_withdrawals TO openvietgis_runtime;
      GRANT UPDATE(status,review_note,reviewed_by,reviewed_at) ON extensions.marketplace_v2_submissions TO openvietgis_runtime;
    """)

def downgrade():
    for table in ("marketplace_v2_withdrawals","marketplace_v2_submissions"):
        op.execute(f"ALTER TABLE extensions.{table} NO FORCE ROW LEVEL SECURITY")
        if op.get_bind().exec_driver_sql(f"SELECT EXISTS(SELECT 1 FROM extensions.{table})").scalar():
            raise RuntimeError("Archive developer release evidence before downgrade")
        op.execute(f"DROP TABLE extensions.{table}")
