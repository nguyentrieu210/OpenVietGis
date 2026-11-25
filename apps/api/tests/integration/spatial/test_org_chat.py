import base64
from uuid import UUID, uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def add_member(org: UUID, name: str) -> UUID:
    user = uuid4()
    with get_admin_engine().begin() as c:
        c.execute(
            text("""INSERT INTO identity.users(id,display_name,email_snapshot,status,created_at,
              updated_at) VALUES(:u,:n,:e,'active',now(),now())"""),
            {"u": user, "n": name, "e": f"{user.hex}@example.test"},
        )
        c.execute(
            text("""INSERT INTO identity.identity_links(id,user_id,issuer,subject,email_snapshot,
              email_verified,display_name_snapshot,created_at) VALUES(:i,:u,'https://issuer.example',
              :s,:e,true,:n,now())"""),
            {"i": uuid4(), "u": user, "s": f"s-{user}", "e": f"{user.hex}@example.test", "n": name},
        )
        c.execute(
            text("""INSERT INTO org.organization_memberships(id,organization_id,user_id,role,status,
              created_at,activated_at) VALUES(:i,:o,:u,'member','active',now(),now())"""),
            {"i": uuid4(), "o": org, "u": user},
        )
    return user


def test_org_chat_rooms_messages_and_contacts(migrated_database):
    world = seed_access_world()
    other = seed_access_world()
    base = f"/api/v1/organizations/{world.organization_id}/chat"
    third = add_member(world.organization_id, "Nguyễn Thị Đào")
    with (
        _client(world.owner_user_id) as owner,
        _client(world.member_user_id) as member,
        _client(third) as dao,
        _client(other.owner_user_id) as stranger,
    ):

        def check(r, status=200):
            assert r.status_code == status, r.text
            return r.json()

        # Org room: lazy, idempotent, every active member is in it.
        rooms = check(owner.get(base + "/rooms"))["items"]
        assert [r["kind"] for r in rooms] == ["org"] and rooms[0]["member_count"] == 3
        org_room = rooms[0]["id"]
        assert check(dao.get(base + "/rooms"))["items"][0]["id"] == org_room
        assert stranger.get(base + "/rooms").status_code in (403, 404)
        assert stranger.get(f"{base}/rooms/{org_room}/messages").status_code in (403, 404)

        # Messages: ascending order, unread, read, reply and mentions.
        first = check(owner.post(f"{base}/rooms/{org_room}/messages", json={"body": " Chào "}), 201)
        assert first["body"] == "Chào" and first["reactions"] == [] and not first["pinned"]
        second = check(
            owner.post(
                f"{base}/rooms/{org_room}/messages",
                json={
                    "body": "@Đào xem",
                    "reply_to_id": first["id"],
                    "mention_ids": [str(third)],
                },
            ),
            201,
        )
        assert second["reply_to"]["id"] == first["id"]
        assert second["mentions"] == [{"id": str(third), "display_name": "Nguyễn Thị Đào"}]
        items = check(dao.get(f"{base}/rooms/{org_room}/messages"))["items"]
        assert [m["id"] for m in items] == [first["id"], second["id"]]
        older = check(dao.get(f"{base}/rooms/{org_room}/messages?before={second['id']}"))
        assert [m["id"] for m in older["items"]] == [first["id"]]
        mine = check(dao.get(base + "/rooms"))["items"][0]
        assert mine["unread"] == 2 and mine["mentioned"] is True
        assert mine["last_message"]["id"] == second["id"]
        assert check(owner.get(base + "/rooms"))["items"][0]["unread"] == 0
        check(dao.post(f"{base}/rooms/{org_room}/read"))
        mine = check(dao.get(base + "/rooms"))["items"][0]
        assert mine["unread"] == 0 and mine["mentioned"] is False
        assert owner.post(f"{base}/rooms/{org_room}/messages", json={"body": "  "}).status_code == 422

        # Image attachments: stored apart, served only to room members.
        png = b"\x89PNG\r\n\x1a\n" + b"\0" * 32
        good = {
            "kind": "image",
            "data_url": "data:image/png;base64," + base64.b64encode(png).decode(),
            "width": 2,
            "height": 2,
        }
        pic = check(
            owner.post(f"{base}/rooms/{org_room}/messages", json={"attachments": [good]}), 201
        )
        assert pic["body"] == "" and pic["attachments"][0]["width"] == 2
        assert "data_url" not in pic["attachments"][0]
        served = dao.get(f"{base}/attachments/{pic['attachments'][0]['id']}")
        assert served.status_code == 200 and served.content == png
        assert served.headers["content-type"] == "image/png"
        assert served.headers["x-content-type-options"] == "nosniff"
        assert stranger.get(f"{base}/attachments/{pic['attachments'][0]['id']}").status_code in (
            403,
            404,
        )
        svg = "data:image/svg+xml;base64," + base64.b64encode(b"<svg/>").decode()
        lying = "data:image/jpeg;base64," + base64.b64encode(png).decode()
        for data_url in (svg, lying, "data:image/png;base64,***"):
            bad_pic = {"attachments": [good | {"data_url": data_url}]}
            r = owner.post(f"{base}/rooms/{org_room}/messages", json=bad_pic)
            assert r.status_code == 422, data_url
        too_many = {"body": "x", "attachments": [good] * 5}
        assert owner.post(f"{base}/rooms/{org_room}/messages", json=too_many).status_code == 422
        check(dao.post(f"{base}/rooms/{org_room}/read"))

        # Reactions toggle.
        url = f"{base}/messages/{first['id']}/reactions"
        assert check(dao.post(url, json={"emoji": "👍"}))["items"] == [
            {"emoji": "👍", "count": 1, "mine": True}
        ]
        assert check(member.post(url, json={"emoji": "👍"}))["items"][0]["count"] == 2
        assert check(dao.post(url, json={"emoji": "👍"}))["items"] == [
            {"emoji": "👍", "count": 1, "mine": False}
        ]
        assert stranger.post(url, json={"emoji": "👍"}).status_code in (403, 404)

        # Pins.
        check(member.post(f"{base}/messages/{first['id']}/pin"))
        pins = check(owner.get(f"{base}/rooms/{org_room}/pins"))["items"]
        assert pins[0]["id"] == first["id"] and pins[0]["pinned_by_name"] == "Member"
        assert check(owner.get(f"{base}/rooms/{org_room}/messages"))["items"][0]["pinned"]
        check(dao.delete(f"{base}/messages/{first['id']}/pin"))
        assert check(owner.get(f"{base}/rooms/{org_room}/pins"))["items"] == []

        # Group: create, non-member 404, invite, rename by admin only, leave.
        bad = {"name": "Tổ đo đạc", "member_ids": [str(other.owner_user_id)]}
        assert owner.post(base + "/rooms", json=bad).status_code == 422
        group = check(
            owner.post(
                base + "/rooms", json={"name": "Tổ đo đạc", "member_ids": [str(world.member_user_id)]}
            ),
            201,
        )
        assert group["kind"] == "group" and group["member_count"] == 2
        gid = group["id"]
        assert dao.get(f"{base}/rooms/{gid}/messages").status_code == 404
        assert dao.post(f"{base}/rooms/{gid}/messages", json={"body": "x"}).status_code == 404
        assert gid not in {r["id"] for r in check(dao.get(base + "/rooms"))["items"]}
        mention_outsider = {"body": "x", "mention_ids": [str(third)]}
        assert member.post(f"{base}/rooms/{gid}/messages", json=mention_outsider).status_code == 422
        assert member.post(
            f"{base}/rooms/{gid}/messages", json={"body": "x", "reply_to_id": first["id"]}
        ).status_code == 422
        check(member.post(f"{base}/rooms/{gid}/members", json={"user_ids": [str(third)]}))
        assert check(dao.get(f"{base}/rooms/{gid}/messages"))["items"] == []
        assert member.patch(f"{base}/rooms/{gid}", json={"name": "Đổi"}).status_code == 403
        assert check(owner.patch(f"{base}/rooms/{gid}", json={"name": "Đổi"}))["name"] == "Đổi"
        assert owner.patch(f"{base}/rooms/{org_room}", json={"name": "x"}).status_code == 422
        assert member.delete(f"{base}/rooms/{gid}/members/{third}").status_code == 403
        assert dao.delete(f"{base}/rooms/{gid}/members/{third}").status_code == 204
        assert dao.get(f"{base}/rooms/{gid}/messages").status_code == 404
        assert owner.delete(f"{base}/rooms/{gid}/members/{world.owner_user_id}").status_code == 204
        # The remaining member is promoted to admin.
        assert check(member.patch(f"{base}/rooms/{gid}", json={"name": "Mới"}))["name"] == "Mới"

        # Direct rooms are unique per pair.
        d1 = check(owner.post(base + "/rooms/direct", json={"user_id": str(third)}))
        d2 = check(dao.post(base + "/rooms/direct", json={"user_id": str(world.owner_user_id)}))
        assert d1["id"] == d2["id"] and d1["kind"] == "direct"
        assert d1["name"] == "Nguyễn Thị Đào" and d1["peer"]["id"] == str(third)
        assert d2["peer"]["display_name"] == "Owner" and d2["peer"]["has_avatar"] is False
        assert owner.post(
            base + "/rooms/direct", json={"user_id": str(world.owner_user_id)}
        ).status_code == 404
        assert owner.post(
            base + "/rooms/direct", json={"user_id": str(other.owner_user_id)}
        ).status_code == 404
        assert member.get(f"{base}/rooms/{d1['id']}/messages").status_code == 404

        # People search: accent and case insensitive, excludes self and other orgs.
        found = check(owner.get(base + "/people", params={"q": "dao"}))["items"]
        assert [p["id"] for p in found] == [str(third)]
        everyone = {p["id"] for p in check(owner.get(base + "/people"))["items"]}
        assert everyone == {str(world.member_user_id), str(third)}

        # Avatars only for members of the same org.
        assert check(owner.get(f"{base}/people/{third}/avatar")) == {"data_url": None}
        assert owner.get(f"{base}/people/{other.owner_user_id}/avatar").status_code == 404
        assert stranger.get(f"{base}/people/{third}/avatar").status_code in (403, 404)

        # Contacts: request, decline, request again, accept, auto-accept, remove.
        assert owner.post(
            base + "/contacts/requests", json={"user_id": str(other.owner_user_id)}
        ).status_code == 404
        req = check(owner.post(base + "/contacts/requests", json={"user_id": str(third)}), 201)
        again = check(owner.post(base + "/contacts/requests", json={"user_id": str(third)}), 201)
        assert req["id"] == again["id"] and req["status"] == "pending"
        incoming = check(dao.get(base + "/contacts"))["incoming"]
        assert [i["id"] for i in incoming] == [req["id"]]
        assert check(owner.get(base + "/contacts"))["outgoing"][0]["user_id"] == str(third)
        assert check(owner.get(base + "/people", params={"q": "đào"}))["items"][0]["request"] == (
            "outgoing"
        )
        assert owner.post(f"{base}/contacts/requests/{req['id']}/accept").status_code == 404
        check(dao.post(f"{base}/contacts/requests/{req['id']}/decline"))
        assert check(dao.get(base + "/contacts"))["incoming"] == []
        req = check(owner.post(base + "/contacts/requests", json={"user_id": str(third)}), 201)
        check(dao.post(f"{base}/contacts/requests/{req['id']}/accept"))
        assert check(owner.get(base + "/contacts"))["items"] == [
            {"user_id": str(third), "display_name": "Nguyễn Thị Đào"}
        ]
        check(member.post(base + "/contacts/requests", json={"user_id": str(third)}), 201)
        auto = check(dao.post(base + "/contacts/requests", json={"user_id": str(world.member_user_id)}), 201)
        assert auto["status"] == "contact"
        assert {c["user_id"] for c in check(dao.get(base + "/contacts"))["items"]} == {
            str(world.owner_user_id),
            str(world.member_user_id),
        }
        assert owner.delete(f"{base}/contacts/{third}").status_code == 204
        assert check(owner.get(base + "/contacts"))["items"] == []
        # The other org's member sees nothing of this org's contacts.
        assert stranger.get(base + "/contacts").status_code in (403, 404)
        assert stranger.get(base + "/people").status_code in (403, 404)

        # CSRF required for writes.
        token = owner.headers.pop("X-OpenVietGIS-CSRF")
        assert owner.post(f"{base}/rooms/{org_room}/messages", json={"body": "x"}).status_code == 403
        owner.headers["X-OpenVietGIS-CSRF"] = token


def test_org_chat_sse_filter():
    from openvietgis_api.adapters.http.org_chat import parse

    org, me = uuid4(), uuid4()
    room = str(uuid4())
    assert parse(f'{{"k":"chat","o":"{org}","r":"{room}"}}', org, me) == ("chat", room)
    assert parse(f'{{"k":"chat","o":"{uuid4()}","r":"{room}"}}', org, me) is None
    assert parse(f'{{"k":"contacts","u":["{me}","{uuid4()}"]}}', org, me) == ("contacts", None)
    assert parse(f'{{"k":"contacts","u":["{uuid4()}"]}}', org, me) is None
    assert parse("not json", org, me) is None
