# R025 — Quickstart (fixture mode)

The first slice is **not a production sync server**. It validates the local format and safely compares two snapshots, without touching a DGN or remote GIS.

```sh
cd integrations/microstation-v8i-bridge
python3 bridge.py validate fixtures/sample-snapshot.json
python3 bridge.py diff fixtures/sample-snapshot.json fixtures/sample-snapshot-v2.json
python3 -m unittest discover -s tests -v
```

To inspect a licensed V8i instance on Windows:

```powershell
py -m pip install pywin32
py bridge.py probe
```

`probe` only attaches to an **already running** COM application and prints metadata. `probe` cannot prove that a drawing's parcels can be extracted, that CRS matches, or that two-way sync works. No credentials are needed; no network call is made.

**Do not upload private cadastral data until an authorized P2 sync API, device pairing, masking and consent controls exist.**
