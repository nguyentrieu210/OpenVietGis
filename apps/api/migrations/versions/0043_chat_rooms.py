"""Spec 060: organisation chat rooms, reactions and contacts."""

from alembic import op

revision = "0043_chat_rooms"
down_revision = "0042_qgis_pairing_scope"
branch_labels = None
depends_on = None

TENANT = "organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid"
ACTOR = "NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid"
TENANT_TABLES = (
    "chat_rooms",
    "chat_room_members",
    "chat_messages",
    "chat_reactions",
    "chat_pins",
    "chat_attachments",
)


def upgrade():
    op.execute("""
    CREATE TABLE collaboration.chat_rooms (
      organization_id uuid NOT NULL REFERENCES org.organizations(id),
      id uuid NOT NULL,
      kind text NOT NULL CHECK (kind IN ('org','group','direct')),
      name varchar(80) CHECK (name IS NULL OR length(btrim(name)) BETWEEN 1 AND 80),
      direct_low uuid REFERENCES identity.users(id),
      direct_high uuid REFERENCES identity.users(id),
      created_by uuid REFERENCES identity.users(id),
      created_at timestamptz NOT NULL DEFAULT now(),
      last_activity_at timestamptz NOT NULL DEFAULT now(),
      PRIMARY KEY (organization_id,id),
      CHECK ((kind='group') = (name IS NOT NULL)),
      CHECK ((kind='direct') = (direct_low IS NOT NULL AND direct_high IS NOT NULL)),
      CHECK (direct_low IS NULL OR direct_low < direct_high)
    );
    CREATE UNIQUE INDEX chat_rooms_one_org ON collaboration.chat_rooms(organization_id)
      WHERE kind='org';
    CREATE UNIQUE INDEX chat_rooms_one_direct ON collaboration.chat_rooms(organization_id,
      direct_low,direct_high) WHERE kind='direct';
    CREATE TABLE collaboration.chat_room_members (
      organization_id uuid NOT NULL, room_id uuid NOT NULL,
      user_id uuid NOT NULL REFERENCES identity.users(id),
      role text NOT NULL DEFAULT 'member' CHECK (role IN ('admin','member')),
      joined_at timestamptz NOT NULL DEFAULT now(), last_read_at timestamptz,
      PRIMARY KEY (organization_id,room_id,user_id),
      FOREIGN KEY (organization_id,room_id) REFERENCES collaboration.chat_rooms(organization_id,id)
        ON DELETE CASCADE
    );
    CREATE INDEX chat_room_members_user ON collaboration.chat_room_members(organization_id,user_id);
    CREATE TABLE collaboration.chat_messages (
      organization_id uuid NOT NULL, room_id uuid NOT NULL, id uuid NOT NULL,
      author_id uuid NOT NULL REFERENCES identity.users(id),
      author_name varchar(200) NOT NULL,
      body text NOT NULL CHECK (length(body)<=4000),
      created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
      reply_to_id uuid,
      mentions jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(mentions)='array'
        AND jsonb_array_length(mentions)<=20),
      PRIMARY KEY (organization_id,id),
      FOREIGN KEY (organization_id,room_id) REFERENCES collaboration.chat_rooms(organization_id,id)
        ON DELETE CASCADE,
      FOREIGN KEY (organization_id,reply_to_id) REFERENCES
        collaboration.chat_messages(organization_id,id)
    );
    CREATE INDEX chat_messages_recent ON collaboration.chat_messages(organization_id,room_id,
      created_at DESC,id DESC);
    CREATE TABLE collaboration.chat_attachments (
      organization_id uuid NOT NULL, id uuid NOT NULL, message_id uuid NOT NULL,
      position smallint NOT NULL CHECK (position BETWEEN 0 AND 3),
      content_type text NOT NULL CHECK (content_type IN ('image/jpeg','image/png','image/webp')),
      width integer NOT NULL CHECK (width BETWEEN 1 AND 4096),
      height integer NOT NULL CHECK (height BETWEEN 1 AND 4096),
      data bytea NOT NULL CHECK (octet_length(data) BETWEEN 1 AND 409600),
      PRIMARY KEY (organization_id,id), UNIQUE (organization_id,message_id,position),
      FOREIGN KEY (organization_id,message_id) REFERENCES
        collaboration.chat_messages(organization_id,id) ON DELETE CASCADE
    );
    CREATE TABLE collaboration.chat_pins (
      organization_id uuid NOT NULL, room_id uuid NOT NULL, message_id uuid NOT NULL,
      pinned_by uuid NOT NULL REFERENCES identity.users(id),
      pinned_by_name varchar(200) NOT NULL,
      pinned_at timestamptz NOT NULL DEFAULT clock_timestamp(),
      PRIMARY KEY (organization_id,message_id),
      FOREIGN KEY (organization_id,message_id) REFERENCES
        collaboration.chat_messages(organization_id,id) ON DELETE CASCADE
    );
    CREATE INDEX chat_pins_room ON collaboration.chat_pins(organization_id,room_id,pinned_at DESC);
    CREATE TABLE collaboration.chat_reactions (
      organization_id uuid NOT NULL, message_id uuid NOT NULL,
      user_id uuid NOT NULL REFERENCES identity.users(id),
      emoji varchar(16) NOT NULL CHECK (length(emoji) BETWEEN 1 AND 16),
      created_at timestamptz NOT NULL DEFAULT now(),
      PRIMARY KEY (organization_id,message_id,user_id,emoji),
      FOREIGN KEY (organization_id,message_id) REFERENCES
        collaboration.chat_messages(organization_id,id) ON DELETE CASCADE
    );
    CREATE TABLE collaboration.chat_contacts (
      user_low uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
      user_high uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
      created_at timestamptz NOT NULL DEFAULT now(),
      PRIMARY KEY (user_low,user_high), CHECK (user_low < user_high)
    );
    CREATE INDEX chat_contacts_high ON collaboration.chat_contacts(user_high);
    CREATE TABLE collaboration.chat_contact_requests (
      id uuid PRIMARY KEY,
      from_user uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
      to_user uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
      created_at timestamptz NOT NULL DEFAULT now(),
      CHECK (from_user <> to_user), UNIQUE (from_user,to_user)
    );
    CREATE INDEX chat_contact_requests_to ON collaboration.chat_contact_requests(to_user);
    """)
    for table in TENANT_TABLES + ("chat_contacts", "chat_contact_requests"):
        op.execute(f"ALTER TABLE collaboration.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE collaboration.{table} FORCE ROW LEVEL SECURITY")
    for table in TENANT_TABLES:
        op.execute(
            f"CREATE POLICY {table}_tenant ON collaboration.{table} FOR ALL TO openvietgis_runtime "
            f"USING ({TENANT} AND {ACTOR} IS NOT NULL) WITH CHECK ({TENANT} AND {ACTOR} IS NOT NULL)"
        )
    op.execute(
        f"CREATE POLICY chat_contacts_self ON collaboration.chat_contacts FOR ALL TO "
        f"openvietgis_runtime USING ({ACTOR} IN (user_low,user_high)) "
        f"WITH CHECK ({ACTOR} IN (user_low,user_high))"
    )
    op.execute(
        f"CREATE POLICY chat_contact_requests_self ON collaboration.chat_contact_requests FOR ALL "
        f"TO openvietgis_runtime USING ({ACTOR} IN (from_user,to_user)) "
        f"WITH CHECK ({ACTOR} IN (from_user,to_user))"
    )
    op.execute(
        "GRANT SELECT,INSERT,UPDATE,DELETE ON collaboration.chat_rooms,"
        "collaboration.chat_room_members,collaboration.chat_messages,collaboration.chat_reactions,"
        "collaboration.chat_pins,collaboration.chat_attachments,"
        "collaboration.chat_contacts,collaboration.chat_contact_requests TO openvietgis_runtime"
    )
    # Members of the verified tenant may read each other's avatar (self policy stays).
    op.execute(f"""CREATE POLICY org_peer_avatar ON identity.user_avatars FOR SELECT TO
      openvietgis_runtime USING ({ACTOR} IS NOT NULL AND EXISTS(
        SELECT 1 FROM org.organization_memberships m WHERE
        m.organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND m.user_id=user_avatars.user_id AND m.status='active'))""")
    # Notify payloads carry ids only, never message text.
    op.execute("""
    CREATE FUNCTION collaboration.publish_chat_room() RETURNS trigger LANGUAGE plpgsql AS $$
    DECLARE o uuid; r uuid;
    BEGIN
      IF TG_TABLE_NAME='chat_reactions' THEN
        IF TG_OP='DELETE' THEN
          SELECT m.organization_id,m.room_id INTO o,r FROM collaboration.chat_messages m
            WHERE m.organization_id=OLD.organization_id AND m.id=OLD.message_id;
        ELSE
          SELECT m.organization_id,m.room_id INTO o,r FROM collaboration.chat_messages m
            WHERE m.organization_id=NEW.organization_id AND m.id=NEW.message_id;
        END IF;
      ELSIF TG_OP='DELETE' THEN
        o := OLD.organization_id; r := OLD.room_id;
      ELSE
        o := NEW.organization_id; r := NEW.room_id;
      END IF;
      IF r IS NOT NULL THEN
        PERFORM pg_notify('ovg_chat',json_build_object('k','chat','o',o,'r',r)::text);
      END IF;
      RETURN NULL;
    END $$;
    CREATE TRIGGER chat_message_event AFTER INSERT ON collaboration.chat_messages
      FOR EACH ROW EXECUTE FUNCTION collaboration.publish_chat_room();
    CREATE TRIGGER chat_reaction_event AFTER INSERT OR DELETE ON collaboration.chat_reactions
      FOR EACH ROW EXECUTE FUNCTION collaboration.publish_chat_room();
    CREATE TRIGGER chat_pin_event AFTER INSERT OR DELETE ON collaboration.chat_pins
      FOR EACH ROW EXECUTE FUNCTION collaboration.publish_chat_room();
    CREATE FUNCTION collaboration.publish_chat_request() RETURNS trigger LANGUAGE plpgsql AS $$
    DECLARE x record;
    BEGIN
      IF TG_OP='DELETE' THEN x := OLD; ELSE x := NEW; END IF;
      PERFORM pg_notify('ovg_chat',json_build_object('k','contacts','u',
        json_build_array(x.from_user,x.to_user))::text);
      RETURN NULL;
    END $$;
    CREATE FUNCTION collaboration.publish_chat_contact() RETURNS trigger LANGUAGE plpgsql AS $$
    DECLARE x record;
    BEGIN
      IF TG_OP='DELETE' THEN x := OLD; ELSE x := NEW; END IF;
      PERFORM pg_notify('ovg_chat',json_build_object('k','contacts','u',
        json_build_array(x.user_low,x.user_high))::text);
      RETURN NULL;
    END $$;
    CREATE TRIGGER chat_contact_event AFTER INSERT OR DELETE ON collaboration.chat_contacts
      FOR EACH ROW EXECUTE FUNCTION collaboration.publish_chat_contact();
    CREATE TRIGGER chat_contact_request_event AFTER INSERT OR DELETE ON
      collaboration.chat_contact_requests
      FOR EACH ROW EXECUTE FUNCTION collaboration.publish_chat_request();
    """)


def downgrade():
    op.execute("DROP POLICY org_peer_avatar ON identity.user_avatars")
    op.execute("""DROP TABLE collaboration.chat_contact_requests, collaboration.chat_contacts,
      collaboration.chat_attachments, collaboration.chat_pins, collaboration.chat_reactions,
      collaboration.chat_messages, collaboration.chat_room_members, collaboration.chat_rooms""")
    op.execute("DROP FUNCTION collaboration.publish_chat_room()")
    op.execute("DROP FUNCTION collaboration.publish_chat_request()")
    op.execute("DROP FUNCTION collaboration.publish_chat_contact()")
