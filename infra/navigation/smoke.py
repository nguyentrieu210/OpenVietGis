"""Provider-only smoke against private loopback endpoints; no production traffic."""
import json
from urllib.parse import urlencode
from urllib.request import urlopen

def get(port, path, params):
    with urlopen(f"http://127.0.0.1:{port}{path}?{urlencode(params)}", timeout=15) as response:
        return json.load(response)

def near_vietnam(p):
    return len(p) >= 2 and 102 <= p[0] <= 110 and 8 <= p[1] <= 24

for query in ("Biên Hòa", "Bien Hoa", "Hà Nội"):
    result = get(12322, "/api/", {"q": query, "limit": 5})
    assert any(near_vietnam(f["geometry"]["coordinates"]) for f in result["features"]), query
result = get(12322, "/reverse", {"lon": 106.842, "lat": 10.95})
assert result["features"], "reverse"
for profile in ("auto", "motorcycle", "pedestrian", "bicycle", "truck"):
    request = {"locations": [{"lat":10.95,"lon":106.842},{"lat":10.96,"lon":106.85}], "costing":profile,"shape_format":"polyline6"}
    trip = get(18002, "/route", {"json": json.dumps(request)})["trip"]
    assert trip["summary"]["length"] > 0 and trip["legs"][0]["shape"], profile
print("PASS: provider search, reverse, five routing profiles")
