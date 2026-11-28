"""Typed, bounded first-party GIS tools. No raw URLs, SQL, credentials or shell."""
import json
from typing import Annotated, Any, Literal
from uuid import UUID

from mcp.types import ToolAnnotations
from pydantic import BaseModel, ConfigDict, Field, ValidationError, model_validator

ID = Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]{1,128}$")]
Revision = Annotated[str, Field(pattern=r"^[a-f0-9]{64}$")]
Bound = Annotated[float, Field(ge=-100000, le=100000, allow_inf_nan=False)]
Operation = Literal["move_batch","copy","offset","rotate","scale","stretch","extend","trim","fillet","intersection","union","difference","centroid","hull","simplify","nearest","spatial_join"]
Check = Literal["duplicate","invalid","overlap","dangling","outside","gap"]

class Args(BaseModel):
    model_config = ConfigDict(extra="forbid")
class Pin(Args):
    layer_id: UUID
    content_revision: Revision
    schema_revision: Revision
class Selection(Pin):
    feature_ids: Annotated[list[ID], Field(max_length=100)] = Field(default_factory=list)
class Layers(Args):
    limit: Annotated[int, Field(ge=1,le=100)] = 50
    after_id: UUID | None = None
class Features(Pin):
    limit: Annotated[int, Field(ge=1,le=50)] = 20
    after_feature_id: ID | None = None
    include_geometry: bool = True
class Feature(Pin):
    feature_id: ID
class Query(Pin):
    attribute_filter:dict[str,Any] | None=None
    spatial_filter:dict[str,Any] | None=None
    sort_field:ID | None=None
    sort_direction:Literal["asc","desc"]="asc"
    limit:Annotated[int,Field(ge=1,le=50)]=20
    offset:Annotated[int,Field(ge=0,le=25000)]=0
class Edit(Args):
    op_type: Literal["create","update","delete"]
    feature_id: ID
    geometry: dict[str,Any] | None = None
    properties: dict[str,Any] | None = None
    @model_validator(mode="after")
    def valid_edit(self):
        if self.op_type=="delete" and (self.geometry is not None or self.properties is not None):
            raise ValueError("Delete must contain ID only")
        if self.op_type=="create" and (self.geometry is None or self.properties is None):
            raise ValueError("Create requires geometry and complete properties")
        if self.op_type=="update" and self.geometry is None and self.properties is None:
            raise ValueError("Update requires geometry or complete properties")
        if self.geometry is not None:
            from live_bridge import clean_result
            self.geometry=clean_result("map_selection",{"crs":"EPSG:4326","geometry":self.geometry})["geometry"]
        return self
class Changeset(Pin):
    operation_id: UUID
    summary: Annotated[str,Field(min_length=1,max_length=200)]
    operations: Annotated[list[Edit],Field(min_length=1,max_length=100)]
    @model_validator(mode="after")
    def distinct(self):
        if len({o.feature_id for o in self.operations})!=len(self.operations):
            raise ValueError("Duplicate feature IDs")
        return self
class Preview(Args):
    operation: Operation
    primary: Selection
    secondary: Selection | None = None
    distance: Bound = 0
    dx: Bound = 0
    dy: Bound = 0
    angle: Annotated[float,Field(ge=-360,le=360,allow_inf_nan=False)] = 0
    factor: Annotated[float,Field(gt=0,le=100,allow_inf_nan=False)] = 1
    vertex: Annotated[int,Field(ge=0,le=4096)] = 1
    @model_validator(mode="after")
    def selected(self):
        if not self.primary.feature_ids:
            raise ValueError("Explicit primary feature IDs required")
        return self
class Topology(Args):
    check: Check
    primary: Selection
    secondary: Selection | None = None
class Overlay(Args):
    title: Annotated[str,Field(min_length=1,max_length=60)]
    geometries: Annotated[list[dict[str,Any]],Field(min_length=1,max_length=50)]
    @model_validator(mode="after")
    def geometry(self):
        from live_bridge import clean_result
        self.geometries=[clean_result("map_selection",{"crs":"EPSG:4326","geometry":g})["geometry"] for g in self.geometries]
        return self
class Visibility(Args):
    layer_id:UUID
    visible:bool
class Select(Selection):
    feature_ids: Annotated[list[ID], Field(min_length=1,max_length=100)]
class Revert(Args):
    changeset_id:UUID
    operation_id:UUID
    summary:Annotated[str,Field(min_length=1,max_length=200)]="Undo AI changeset"

class FieldSpec(Args):
    field_id:Annotated[str,Field(pattern=r"^[A-Za-z][A-Za-z0-9_]{0,63}$")]
    name:Annotated[str,Field(min_length=1,max_length=120)]
    type:Literal["text","integer","decimal","boolean","timestamp"]
    nullable:bool
class ManualLayer(Args):
    operation_id:UUID
    name:Annotated[str,Field(min_length=1,max_length=120)]
    geometry_family:Literal["Point","LineString","Polygon"]
    fields:Annotated[list[FieldSpec],Field(max_length=100)] = Field(default_factory=list)
class Schema(Args):
    primary:Selection
    action:Literal["add","rename"]
    field_id:Annotated[str,Field(pattern=r"^[A-Za-z][A-Za-z0-9_]{0,63}$")]
    name:Annotated[str,Field(min_length=1,max_length=120)]
    type:Literal["text","integer","decimal","boolean","timestamp"]="text"
class Export(Pin):
    format:Literal["geojson"]="geojson"

MODELS={"gis_create_layer":ManualLayer,"gis_schema":Schema,"gis_export":Export,"gis_layers":Layers,"gis_features":Features,"gis_feature":Feature,"gis_query":Query,"gis_changeset":Changeset,
        "gis_geometry_preview":Preview,"gis_topology":Topology,"gis_overlay":Overlay,
        "gis_layer_visibility":Visibility,"gis_select":Select,"gis_revert":Revert}
from advanced_tools import TOOLS as ADVANCED_TOOLS

MODELS.update({name:spec[0] for name,spec in ADVANCED_TOOLS.items()})
METHODS=frozenset(MODELS)
def validate(method, params):
    from live_bridge import BridgeError
    try:
        data=MODELS[method].model_validate(params).model_dump(mode="json",exclude_none=True)
        if len(json.dumps(data,ensure_ascii=False,allow_nan=False).encode())>85000:
            raise ValueError("Command exceeds size limit")
        return data
    except (ValidationError,ValueError,TypeError,KeyError,BridgeError,RecursionError):
        raise BridgeError(422,"Invalid or oversized GIS workspace command") from None

def result(value):
    from live_bridge import BridgeError
    if not isinstance(value,dict):
        raise BridgeError(422,"Workspace result must be an object")
    try:
        if len(json.dumps(value,ensure_ascii=False,allow_nan=False).encode())>85000:
            raise ValueError()
    except (ValueError,TypeError,RecursionError):
        raise BridgeError(422,"GIS result exceeds 85 KB; use a smaller page") from None
    return value

def register(mcp,operate):
    def tool(name,model,description,readonly=True,destructive=False):
        async def run(request, session_id: UUID | None = None):
            return await operate(name,validate(name,request.model_dump(mode="json",exclude_none=True)),session_id)
        run.__name__=name
        run.__doc__=description
        run.__annotations__={"request":model,"session_id":UUID|None,"return":dict[str,Any]}
        mcp.tool(annotations=ToolAnnotations(read_only_hint=readonly,destructive_hint=destructive),structured_output=True)(run)
    tool("gis_create_layer",ManualLayer,"Create an editable WGS84 vector layer with attribute schema; stable operation UUID.",False)
    tool("gis_schema",Schema,"Add or rename a field on a pinned source.",False)
    tool("gis_export",Export,"Export a pinned layer as GeoJSON to the user's browser downloads.",False)
    tool("gis_layers",Layers,"List active project layers, schemas, capabilities and revision pins. Start here.")
    tool("gis_features",Features,"Page features and attributes from a pinned layer; WGS84 geometry. Small pages for large geometry.")
    tool("gis_query",Query,"Filter by typed attribute AST (e.g. {op: eq, field: name, value: A}) or spatial predicate with WGS84 geometry; no raw SQL.")
    tool("gis_feature",Feature,"Read one feature with current content/schema pins.")
    tool("gis_geometry_preview",Preview,"PostGIS CAD and analysis preview: move, copy, offset, rotate, scale, stretch, extend, trim, fillet, intersection, union, difference, centroid, hull, simplify, nearest, spatial join. No persistence. Use gis_changeset to commit reviewed output.")
    tool("gis_topology",Topology,"Find duplicates, invalid geometry, overlaps, dangles, outside or gaps. No persistence.")
    tool("gis_overlay",Overlay,"Draw temporary WGS84 geometry on the live map; no persistent editing.",False)
    tool("gis_layer_visibility",Visibility,"Show or hide a project layer in the current workspace.",False)
    tool("gis_select",Select,"Select loaded, visible objects by layer and pinned feature IDs.",False)
    tool("gis_changeset",Changeset,"Atomically create, update or delete up to 100 features on one editable layer. WGS84 geometry; properties replace the full attribute map. Use current pins and a stable unique operation UUID. Requires explicit GIS approval.",False,True)
    for name,(model,description,readonly,destructive) in ADVANCED_TOOLS.items():
        tool(name,model,description,readonly,destructive)
    tool("gis_revert",Revert,"Revert a specific GIS changeset using a new operation UUID. Requires explicit GIS approval and server authorization.",False,True)
