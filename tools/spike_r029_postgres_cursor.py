"""R029 disposable Postgres transaction-order spike (NOT production migration).
Requires a dedicated no-network container created explicitly for this test.
"""
import subprocess
import time

NAME = "ovg-r029-cursor-test-283070f"
BASE = ["docker", "exec", "-i", NAME, "psql", "-X", "-U", "postgres", "-d", "r029_test", "-v", "ON_ERROR_STOP=1", "-Atq"]

def query(sql):
    return subprocess.run(BASE, input=sql, text=True, capture_output=True, check=True).stdout.strip()

def start():
    return subprocess.Popen(BASE, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

def send(process, sql):
    process.stdin.write(sql)
    process.stdin.close()

def finish(process):
    assert process.wait(timeout=15) == 0, process.stderr.read()
    return process.stdout.read().strip()

def wait_for(marker, state):
    for _ in range(100):
        if query("SELECT count(*) FROM pg_stat_activity WHERE application_name='"+marker+"' AND "+state+";") == "1":
            return
        time.sleep(.05)
    raise AssertionError("session did not reach expected state: "+marker)

def main():
    assert query("SELECT current_database();") == "r029_test"
    assert query("SELECT current_setting('server_version_num');").isdigit()
    query("""
    CREATE SCHEMA r029_spike;
    CREATE TABLE r029_spike.project(id int PRIMARY KEY, cursor bigint NOT NULL DEFAULT 0);
    CREATE TABLE r029_spike.event(project int NOT NULL, cursor bigint NOT NULL, payload text NOT NULL,
      PRIMARY KEY(project,cursor));
    INSERT INTO r029_spike.project(id) VALUES (1),(2);
    CREATE FUNCTION r029_spike.append_event(p int, msg text) RETURNS bigint LANGUAGE plpgsql AS $$
    DECLARE n bigint;
    BEGIN
      UPDATE r029_spike.project SET cursor=cursor+1 WHERE id=p RETURNING cursor INTO STRICT n;
      INSERT INTO r029_spike.event VALUES(p,n,msg);
      RETURN n;
    END $$;
    """)
    a = start()
    send(a, "SET application_name='r029_a'; BEGIN; SELECT r029_spike.append_event(1,'A'); SELECT pg_sleep(2); COMMIT;")
    wait_for("r029_a", "wait_event='PgSleep'")
    b = start()
    send(b, "SET application_name='r029_b'; BEGIN; SELECT r029_spike.append_event(1,'B'); COMMIT;")
    wait_for("r029_b", "wait_event_type='Lock'")
    finish(a)
    finish(b)
    assert query("SELECT string_agg(cursor::text||':'||payload,',' ORDER BY cursor) FROM r029_spike.event WHERE project=1;") == "1:A,2:B"
    print("PASS same-project writers serialized in commit order")
    query("BEGIN; SELECT r029_spike.append_event(1,'rolled back'); ROLLBACK;")
    assert query("SELECT r029_spike.append_event(1,'C');") == "3"
    assert query("SELECT count(*) FROM r029_spike.event WHERE payload='rolled back';") == "0"
    print("PASS rollback removes event and cursor advance")
    assert query("SELECT r029_spike.append_event(2,'other');") == "1"
    print("PASS independent project cursor")
    reader = start()
    send(reader, "SET application_name='r029_reader'; BEGIN; SELECT cursor FROM r029_spike.project WHERE id=1 FOR SHARE; SELECT pg_sleep(2); SELECT count(*) FROM r029_spike.event WHERE project=1; COMMIT;")
    wait_for("r029_reader", "wait_event='PgSleep'")
    writer = start()
    send(writer, "SET application_name='r029_writer'; SELECT r029_spike.append_event(1,'D');")
    wait_for("r029_writer", "wait_event_type='Lock'")
    out = finish(reader).splitlines()
    finish(writer)
    assert [x for x in out if x] == ["3","3"], out
    assert query("SELECT cursor FROM r029_spike.project WHERE id=1;") == "4"
    print("PASS snapshot watermark held against concurrent writer")
    print("4/4 isolated PostgreSQL checks passed")

if __name__ == "__main__":
    main()
