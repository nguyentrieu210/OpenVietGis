"""Deterministic batch numbering on route geometry, with virtual occupancy."""
from __future__ import annotations

from collections import Counter, defaultdict
from typing import Any

from modules.house_numbering.domain import (
    Address,
    Rule,
    _parse,
    _prefix,
    locate_on_route,
    suggest_number,
)

PROTECTED = {"submitted","approved","reserved"}
STATES = {"old","certificate","cad","survey","proposed","submitted","approved","reserved"}


def build_route_plan(*, route: dict[str,Any], vertices: list[tuple[float,float]], rule: Rule,
                     points: list[dict[str,Any]], numbers: list[dict[str,Any]],
                     targets: dict[str,dict[str,Any]]) -> tuple[list[dict[str,Any]],list[str]]:
    """Compute one coherent proposal without writing or renumbering existing anchors."""
    warnings=[]
    changes=[]
    locations={}
    primary_ids=set()
    addresses=[]
    occupancy: dict[str,set[str | None]]=defaultdict(set)
    numbers_by_building: dict[str,list[dict[str,Any]]]=defaultdict(list)
    for n in numbers:
        bid=str(n["building_id"]) if n["building_id"] is not None else None
        occupancy[str(n["number_text"])].add(bid)
        if bid is not None:
            numbers_by_building[bid].append(n)
    for row in points:
        bid=str(row["id"])
        location=locate_on_route((float(row["x"]),float(row["y"])),vertices,crs=f'EPSG:{route["srid"]}')
        locations[bid]=(location,row)
        if location.side is not None and location.distance <= rule.max_distance and row.get("number_text"):
            number_id=str(row["number_id"])
            primary_ids.add(number_id)
            state=row.get("kind") or "proposed"
            addresses.append(Address(number_id,str(row["number_text"]),location.measure,location.side,state if state in STATES else "proposed"))
    # Evidence and reservations not chosen as physical anchors still occupy their numbers.
    for n in numbers:
        if str(n["id"]) not in primary_ids:
            addresses.append(Address(str(n["id"]),str(n["number_text"]),float(n["measure_m"] or 0),n["side"] if n["side"] in {"left","right"} else "left","reserved"))
    sorted_targets=[]
    for bid,building in targets.items():
        if building["state"]!="active" or building["eligibility"]=="ineligible":
            warnings.append(f"building {bid}: ineligible_building")
        elif bid not in locations:
            warnings.append(f"building {bid}: invalid_geometry")
        elif building.get("protected_number") or any(n["kind"] in PROTECTED for n in numbers_by_building[bid]):
            warnings.append(f"building {bid}: protected_number")
        elif locations[bid][0].side is None:
            warnings.append(f"building {bid}: ambiguous_route_side")
        elif locations[bid][0].distance>rule.max_distance:
            warnings.append(f"building {bid}: outside_route_distance")
        else:
            sorted_targets.append(bid)
    # Equal chainage on one side is ambiguous; do not invent an entrance order.
    locations_count=Counter((locations[b][0].side,round(locations[b][0].measure,6)) for b in sorted_targets)
    sorted_targets.sort(key=lambda b:(locations[b][0].side,locations[b][0].measure,b))
    prefix=str(route.get("prefix") or "")
    for bid in sorted_targets:
        loc,row=locations[bid]
        if locations_count[(loc.side,round(loc.measure,6))]>1:
            warnings.append(f"building {bid}: ambiguous_location")
            continue
        # Historical evidence is preserved. A proposed number already present
        # needs no replacement. A duplicate/malformed evidence number needs review.
        existing=str(row["number_text"]) if row.get("number_text") else None
        if existing and row["kind"]=="proposed":
            warnings.append(f"building {bid}: already_proposed")
            continue
        if existing:
            parsed=_parse(existing,_prefix(prefix),rule.suffix_style)
            if not parsed or parsed[0]%2!=rule.parity(loc.side) or occupancy[existing]-{bid}:
                warnings.append(f"building {bid}: conflicting_existing_number")
                continue
            physical = sorted((a for a in addresses if a.side==loc.side and a.state!="reserved"),key=lambda a:(a.measure,a.id))
            parsed_anchors=[_parse(a.number,_prefix(prefix),rule.suffix_style) for a in physical]
            if any(v is None or v[0]%2!=rule.parity(loc.side) for v in parsed_anchors) or any(
                a.measure>=b.measure or av>=bv
                for a,b,av,bv in zip(physical,physical[1:],parsed_anchors,parsed_anchors[1:])
            ):
                warnings.append(f"building {bid}: nonmonotonic_or_duplicate_anchor")
                continue
            number,reason=existing,"existing_number"
        else:
            proposal=suggest_number(addresses,measure=loc.measure,side=loc.side,rule=rule,prefix=prefix,distance=loc.distance,batch_before_first=True)
            if proposal.number is None:
                warnings.append(f"building {bid}: {proposal.reason}")
                continue
            number,reason=proposal.number,proposal.reason
            if occupancy[number]:
                warnings.append(f"building {bid}: number_occupied")
                continue
            addresses.append(Address("batch:"+bid,number,loc.measure,loc.side,"proposed"))
            occupancy[number].add(bid)
        changes.append({"building_id":bid,"route_id":str(route["id"]),"old_number":existing,"number":number,
                        "side":loc.side,"measure":loc.measure,"reason":reason})
    return changes,warnings
