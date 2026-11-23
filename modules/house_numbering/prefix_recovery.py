"""Read-only recovery queue for address-prefix conflicts. Never invent geometry."""
from __future__ import annotations
from collections import Counter, defaultdict
import json
from typing import Any
from sqlalchemy import text
from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.building_assignment import alley_name, road_name
from modules.house_numbering.route_recovery import MAX_JUNCTION_M

def address_chain(prefix: str) -> list[str]:
    if not prefix:
        return [""]
    parts=canonical_prefix(prefix).lower().split("/")
    return [""]+["/".join(parts[:i]) for i in range(1,len(parts)+1)]

def valid_parent(idx: Any, parent_id: str, root: str, chain: list[str]) -> bool:
    """Check the actual ancestor IDs, not uniqueness of street labels."""
    current=parent_id
    for expected in reversed(chain):
        r=idx.routes.get(current)
        if not r or idx.root_name.get(current)!=root or alley_name(r["prefix"])!=expected:
            return False
        current=str(r["parent_route_id"] or "")
    return not current

def prefix_plan(session: Any, params: dict[str,Any], idx: Any, source_paths: dict, groups: dict, search: str, offset: int, limit: int) -> dict:
    paths=defaultdict(list)
    for rid,root in idx.root_name.items():
        r=idx.routes[rid]
        paths[(root,alley_name(r["prefix"]))].append({
            "route_id":rid,"source_id":str(r["linked_source_id"]),"feature_id":r["linked_feature_id"],"name":r["name"]})
    results=[]; specs=[]; pairs=[]; by_id={}
    for g in groups.values():
        row={k:v for k,v in g.items() if k!="building_ids"}
        row["count"]=len(g["building_ids"])
        row["codes"]=[{"code":k,"count":n} for k,n in g["codes"].most_common(10)]
        row["old_routes"]=list(g["old_routes"].values())[:10]
        row["chain"]=[]; row["sources"]=[];row["near"]=0;row["junction_m"]=None
        row["parent_candidates"]=[];row["geometry_connected"]=False;row["status"]="missing_geometry"
        try: chain=address_chain(g["alley"])
        except ValueError:
            row["status"]="invalid_prefix";results.append(row);continue
        for p in chain:
            key=(g["normalized_road"],p)
            saved=paths.get(key,[]); pending=source_paths.get(key,[])
            row["chain"].append({"prefix":p,"existing_count":len(saved),"source_count":len(pending),
                "existing":saved[:10],"sources":pending[:10]})
        leaf=row["chain"][-1]
        pending=source_paths.get((g["normalized_road"],chain[-1]),[])
        if leaf["existing_count"]:
            row["status"]="existing_path_conflict"
        elif not pending:
            row["status"]="missing_geometry"
        elif len(pending)>200:
            row["status"]="ambiguous_geometry"
        else:
            row["sources"]=pending[:10]
            specs.append({"id":g["id"],"links":pending,"building_ids":g["building_ids"]})
            if len(chain)==1:
                row["status"]="root_review"
            else:
                parents=[p for p in paths.get((g["normalized_road"],chain[-2]),[]) if valid_parent(idx,p["route_id"],g["normalized_road"],chain[:-1])]
                if 0<len(parents)<=100:
                    for p in parents:pairs.append({"id":g["id"],"parent_id":p["route_id"]})
                    row["status"]="junction_pending"
                else:
                    row["status"]="ambiguous_parent" if parents else "parent_missing"
        results.append(row);by_id[g["id"]]=row
    cte="""
        WITH specs AS MATERIALIZED (SELECT v->>'id' id,v->'links' links,v->'building_ids' ids FROM jsonb_array_elements(CAST(:specs AS jsonb)) v),
        merged AS MATERIALIZED (
          SELECT s.id,house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) geom
          FROM specs s CROSS JOIN LATERAL jsonb_array_elements(s.links) WITH ORDINALITY l(v,ord)
          JOIN spatial.reference_features f ON f.organization_id=:org AND f.project_id=:project AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
          GROUP BY s.id HAVING count(*)=max(jsonb_array_length(s.links))
        ), lines AS MATERIALIZED (SELECT id,ST_Transform(geom,4326)::geography g FROM merged WHERE geom IS NOT NULL)
    """
    if specs:
        for v in session.execute(text(cte+"""
            SELECT s.id,l.id IS NOT NULL connected,count(*) FILTER (WHERE b.eligibility<>'ineligible' AND
                ST_DWithin(ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography,l.g,35)) near
            FROM specs s LEFT JOIN lines l ON s.id=l.id
            CROSS JOIN LATERAL jsonb_array_elements_text(s.ids) bid(val)
            JOIN house_numbering.buildings b ON b.organization_id=:org AND b.project_id=:project AND b.id=bid.val::uuid
            JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
            GROUP BY s.id,l.id
        """),{**params,"specs":json.dumps(specs)}).mappings():
            row=by_id[v["id"]];row["near"]=v["near"];row["geometry_connected"]=v["connected"]
            if not v["connected"]:row["status"]="ambiguous_geometry"
    if pairs:
        for v in session.execute(text(cte+"""
            , comparisons AS (SELECT * FROM jsonb_to_recordset(CAST(:pairs AS jsonb)) AS x(id text,parent_id uuid))
            SELECT c.id,c.parent_id,LEAST(ST_Distance(ST_StartPoint(a.g::geometry)::geography,ST_Transform(l.geometry,4326)::geography),
              ST_Distance(ST_EndPoint(a.g::geometry)::geography,ST_Transform(l.geometry,4326)::geography)) distance
            FROM comparisons c JOIN lines a ON a.id=c.id JOIN house_numbering.route_lines l ON l.organization_id=:org AND l.project_id=:project AND l.route_id=c.parent_id
        """),{**params,"specs":json.dumps(specs),"pairs":json.dumps(pairs)}).mappings():
            row=by_id[v["id"]];d=v["distance"];r=idx.routes[str(v["parent_id"])]
            row["parent_candidates"].append({"route_id":str(v["parent_id"]),"name":r["name"],"source_id":str(r["linked_source_id"]),
                  "feature_id":r["linked_feature_id"],"distance_m":round(d,2) if d is not None else None,
                  "connected":d is not None and d<=MAX_JUNCTION_M})
    counts=Counter()
    for row in results:
        candidates=row["parent_candidates"]
        candidates.sort(key=lambda r:(not r["connected"],r["distance_m"] if r["distance_m"] is not None else float("inf"),r["route_id"]))
        connected=[p for p in candidates if p["connected"]]
        row["parent_candidates"]=candidates[:10]
        if row["status"]=="junction_pending":
            row["junction_m"]=candidates[0]["distance_m"] if candidates else None
            row["status"]="connected_review" if len(connected)==1 and row["near"] else "ambiguous_parent" if len(connected)>1 else "disconnected" if not connected else "distance"
        if row["status"]=="root_review" and not row["near"]:row["status"]="distance"
        counts[row["status"]]+=row["count"]
    counts["total"]=sum(row["count"] for row in results)
    needle=road_name(search)
    priority={"connected_review":0,"root_review":1,"parent_missing":2,"ambiguous_geometry":3,"ambiguous_parent":4}
    filtered=[r for r in results if not needle or needle in road_name(r["road"]+" "+r["alley"]+" "+" ".join(x["code"] for x in r["codes"]))]
    filtered.sort(key=lambda r:(priority.get(r["status"],5),-r["near"],-r["count"],r["id"]))
    return {"counts":dict(counts),"total_groups":len(results),"filtered_groups":len(filtered),
      "items":filtered[offset:offset+limit],"next_offset":offset+limit if len(filtered)>offset+limit else None}
