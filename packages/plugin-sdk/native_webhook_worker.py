"""Platform-operated delivery of the transactional OpenVietGIS event journal.

One isolated worker/state DB per approved project + destination; credentials and
HTTPS destination are provisioned by the platform operator, not by public users.
"""
from __future__ import annotations

import http.client
import ipaddress
import json
import os
import re
import socket
import sqlite3
import ssl
import time
import urllib.parse
import urllib.request
from uuid import UUID

from webhook_relay import OPENER, event_body, https_url, signature


def public_target(url):
    parts=urllib.parse.urlsplit(https_url(url))
    host=parts.hostname or ""
    if parts.netloc!=host or not re.fullmatch(r"(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?[.])+[a-z]{2,63}",host):
        raise ValueError("webhook target must use a public DNS hostname and default HTTPS port")
    answers=socket.getaddrinfo(host,443,socket.AF_UNSPEC,socket.SOCK_STREAM)
    candidates=[]
    for family,_,_,_,address in answers:
        ip=ipaddress.ip_address(address[0])
        if not ip.is_global or ip.is_multicast or ip.is_reserved or ip.is_loopback or ip.is_link_local or getattr(ip,"ipv4_mapped",None):
            raise ValueError("non-public webhook address")
        if family==socket.AF_INET:
            candidates.append(str(ip))
    if not candidates:
        raise ValueError("no public IPv4 webhook address")
    return host,sorted(set(candidates))[0],parts.path or "/"


def pinned_post(url,body,headers):
    host,ip,path=public_target(url)
    class PinnedConnection(http.client.HTTPSConnection):
        def connect(self):
            raw=socket.create_connection((ip,443),timeout=self.timeout)
            try:
                self.sock=ssl.create_default_context().wrap_socket(raw,server_hostname=host)
            except BaseException:
                raw.close()
                raise
    conn=PinnedConnection(host,443,timeout=10)
    try:
        conn.request("POST",path,body=body,headers=headers)
        response=conn.getresponse()
        if not 200<=response.status<300:
            raise RuntimeError("delivery rejected")
    finally:
        conn.close()


class NativeWorker:
    def __init__(self, db, project, page, deliver, *, replay=False):
        self.db, self.project, self.page, self.deliver = db,str(UUID(project)),page,deliver
        self.replay=replay
        db.executescript("""
        CREATE TABLE IF NOT EXISTS native_state(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS native_queue(
          sequence INTEGER PRIMARY KEY,id TEXT NOT NULL UNIQUE,body BLOB NOT NULL,delivered INTEGER NOT NULL DEFAULT 0);
        """)
        existing=db.execute("SELECT value FROM native_state WHERE key='project'").fetchone()
        if existing and existing[0]!=self.project:
            raise ValueError("state belongs to another project")
        with db:
            db.execute("INSERT OR IGNORE INTO native_state VALUES('project',?)",(self.project,))

    def poll(self):
        row=self.db.execute("SELECT value FROM native_state WHERE key='checkpoint'").fetchone()
        after=int(row[0]) if row else 0
        page=self.page(after)
        head=page["head_sequence"]
        items=page["items"]
        if not isinstance(items,list) or len(items)>200 or type(head) is not int or head<after:
            raise ValueError("invalid event journal or checkpoint")
        if not row and not self.replay:
            with self.db:
                self.db.execute("INSERT INTO native_state VALUES('checkpoint',?)",(str(head),))
            return 0
        last=after
        entries=[]
        for event in items:
            seq=event["sequence"]
            if type(seq) is not int or not last<seq<=head:
                raise ValueError("non-monotonic event sequence")
            last=seq
            body=event_body(event,self.project)
            if body is None:
                raise ValueError("unknown journal event")
            value=json.loads(body)
            value["schema"]="openvietgis.integration.event.v1"
            value["sequence"]=seq
            entries.append((seq,value["id"],json.dumps(value,sort_keys=True,separators=(",",":")).encode()))
        if page["next_after_sequence"]!=last:
            raise ValueError("invalid continuation")
        with self.db:
            self.db.executemany("INSERT OR IGNORE INTO native_queue(sequence,id,body) VALUES(?,?,?)",entries)
            self.db.execute("INSERT INTO native_state VALUES('checkpoint',?) "
                            "ON CONFLICT(key) DO UPDATE SET value=excluded.value",(str(last),))
        return len(entries)

    def flush(self,limit=100):
        rows=self.db.execute("SELECT sequence,id,body FROM native_queue WHERE delivered=0 ORDER BY sequence LIMIT ?",(limit,)).fetchall()
        for seq,event_id,body in rows:
            self.deliver(event_id,body)
            with self.db:
                self.db.execute("UPDATE native_queue SET delivered=1 WHERE sequence=?",(seq,))
        return len(rows)


def main():
    root=https_url(os.environ["OVG_API_BASE"])
    org,project=(str(UUID(os.environ[k])) for k in ("OVG_ORG_ID","OVG_PROJECT_ID"))
    token=os.environ["OVG_SERVICE_TOKEN"]
    target=https_url(os.environ["WEBHOOK_URL"])
    secret=os.environ["WEBHOOK_SECRET"].encode()
    if len(secret)<32:
        raise ValueError("WEBHOOK_SECRET must be at least 32 bytes")
    state=os.environ["RELAY_STATE_DB"]
    fd=os.open(state,os.O_CREAT|os.O_WRONLY,0o600)
    os.close(fd)
    if os.stat(state).st_mode&0o077:
        raise ValueError("state file must be private")
    endpoint=f"{root}/organizations/{org}/projects/{project}/integrations/events"
    interval=max(5,min(300,int(os.environ.get("POLL_SECONDS","10"))))
    def page(after):
        req=urllib.request.Request(endpoint+"?"+urllib.parse.urlencode({"after":after,"limit":200}),
            headers={"Authorization":"Bearer "+token,"Accept":"application/json"})
        with OPENER.open(req,timeout=10) as res:
            raw=res.read(1_000_001)
            if len(raw)>1_000_000:
                raise ValueError("journal page too large")
            return json.loads(raw)
    def deliver(event_id,body):
        stamp=str(int(time.time()))
        pinned_post(target,body,{"Content-Type":"application/json","X-OVG-Event-Id":event_id,
            "X-OVG-Timestamp":stamp,"X-OVG-Signature":signature(secret,stamp,body)})
    with sqlite3.connect(state) as db:
        worker=NativeWorker(db,project,page,deliver,replay=os.environ.get("REPLAY_JOURNAL")=="1")
        failures=0
        while True:
            try:
                # Re-check project credential before flushing even an existing queue.
                n=worker.poll()
                delivered=worker.flush()
                failures=0
                if n<200 and delivered<100:
                    time.sleep(interval)
            except (OSError,ValueError,RuntimeError,KeyError,TypeError) as exc:
                failures+=1
                print(f"Webhook delivery paused ({type(exc).__name__}); retry {failures}",flush=True)
                time.sleep(min(300,interval*2**min(failures,4)))


if __name__=="__main__":
    main()
