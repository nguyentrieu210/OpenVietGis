"""Typed advanced GIS tool contracts. Core API remains authoritative."""
from typing import Annotated,Literal,Any
from uuid import UUID
from pydantic import BaseModel,ConfigDict,Field,model_validator

class Args(BaseModel):
    model_config=ConfigDict(extra="forbid",allow_inf_nan=False)
Name=Annotated[str,Field(min_length=1,max_length=120)]
Revision=Annotated[int,Field(ge=1,le=2147483646)]
Key=Annotated[str,Field(pattern=r"^[a-zA-Z0-9_.-]{1,128}$")]
Band=Annotated[int,Field(ge=1,le=16)]
RasterMethod=Literal["band","ndvi","ndwi","ndbi","hillshade","calculator","rgb","clip","change","slope","aspect","mosaic","resample"]
class Empty(Args):pass
class Raster(Args):
    method:RasterMethod
    name:Name
    source_id:UUID
    second_id:UUID|None=None
    third_id:UUID|None=None
    band_a:Band=1
    band_b:Band=2
    band_c:Band=3
    formula:Annotated[str,Field(min_length=1,max_length=200)]|None=None
    clip_geojson:dict[str,Any]|None=None
    factor:Annotated[float,Field(ge=.1,le=10)]|None=None
    resampling:Literal["near","bilinear"]|None=None
    @model_validator(mode="after")
    def check(self):
        if self.second_id and self.method not in ("ndvi","ndwi","ndbi","calculator","rgb","change","mosaic"):raise ValueError("Unexpected second input")
        if self.third_id and self.method!="rgb":raise ValueError("Unexpected third input")
        if self.method=="mosaic" and not self.second_id:raise ValueError("Mosaic requires second raster")
        if (self.formula is not None)!=(self.method=="calculator") or (self.clip_geojson is not None)!=(self.method=="clip"):raise ValueError("Invalid formula or clip")
        if (self.factor is not None)!=(self.method=="resample") or (self.resampling is not None)!=(self.method=="resample"):raise ValueError("Resample requires factor and method")
        return self
class RasterReport(Args):
    method:Literal["zonal","contour"]
    source_id:UUID
    band:Band=1
    zones:Annotated[list[dict[str,Any]],Field(min_length=1,max_length=100)]|None=None
    interval:Annotated[float,Field(gt=0)]|None=None
    @model_validator(mode="after")
    def check(self):
        if (self.zones is not None)!=(self.method=="zonal") or (self.interval is not None)!=(self.method=="contour"):raise ValueError("Provide zones or interval")
        return self
class RasterId(Args):layer_id:UUID
class Proc(Args):
    algorithm_id:Literal["buffer","clip","dissolve"]
    input_layer_id:UUID
    overlay_layer_id:UUID|None=None
    output_name:Name
    operation_id:UUID
    parameters:dict[str,str|int|float|None]=Field(default_factory=dict)
    @model_validator(mode="after")
    def check(self):
        if (self.overlay_layer_id is not None)!=(self.algorithm_id=="clip"):raise ValueError("Clip requires overlay")
        return self
class Job(Args):job_id:UUID
class Ref(Args):
    type:Literal["layer","step"]
    id:UUID
class Node(Args):
    id:UUID
    algorithm:Literal["buffer","clip","dissolve","band","ndvi","ndwi","ndbi","hillshade","slope","aspect","resample","mosaic","change","rgb","calculator"]
    name:Name
    a:Ref
    b:Ref|None=None
    c:Ref|None=None
    parameters:dict[str,str|int|float|None]=Field(default_factory=dict)
    x:Annotated[int,Field(ge=0,le=5000)]=0
    y:Annotated[int,Field(ge=0,le=5000)]=0
class Graph(Args):
    version:Literal[1]=1
    nodes:Annotated[list[Node],Field(min_length=1,max_length=12)]
    @model_validator(mode="after")
    def dag(self):
        by={n.id:n for n in self.nodes}
        if len(by)!=len(self.nodes):raise ValueError("Duplicate steps")
        vector={"buffer","clip","dissolve"}
        pending=set(by);done=set()
        for n in self.nodes:
            for r in (n.a,n.b,n.c):
                if r and r.type=="step" and (r.id not in by or ((by[r.id].algorithm in vector)!=(n.algorithm in vector))):raise ValueError("Invalid step reference")
        while pending:
            ready={i for i in pending if all(r is None or r.type=="layer" or r.id in done for r in (by[i].a,by[i].b,by[i].c))}
            if not ready:raise ValueError("Cycle in graph")
            pending-=ready;done|=ready
        return self
class Saved(Args):
    name:Name
    id:UUID|None=None
    revision:Revision|None=None
    @model_validator(mode="after")
    def check(self):
        if (self.id is None)!=(self.revision is None):raise ValueError("Update requires ID and revision")
        return self
class ModelSave(Saved):graph:Graph
class ModelRun(Args):
    model_id:UUID
    revision:Revision
    run_id:UUID
class Camera(Args):
    lng:Annotated[float,Field(ge=-180,le=180)]
    lat:Annotated[float,Field(ge=-85,le=85)]
    zoom:Annotated[float,Field(ge=0,le=24)]
    bearing:Annotated[float,Field(ge=-180,le=180)]=0
    reference_width:Annotated[float,Field(ge=1,le=100000)]=900
class Element(Args):
    id:UUID
    kind:Literal["map","text","legend","north","scale","rect"]
    x:Annotated[float,Field(ge=0,le=1200)]
    y:Annotated[float,Field(ge=0,le=1200)]
    w:Annotated[float,Field(ge=5,le=1200)]
    h:Annotated[float,Field(ge=5,le=1200)]
    text:Annotated[str,Field(max_length=500)]=""
    font_size:Annotated[float,Field(ge=2,le=40)]=4
    color:Annotated[str,Field(pattern=r"^#[0-9a-fA-F]{6}$")]="#15383b"
    border:bool=True
    grid:bool=False
    camera:Camera|None=None
    map_id:UUID|None=None
class Layout(Args):
    version:Literal[1]=1
    paper:Literal["A0","A1","A2","A3","A4"]="A4"
    landscape:bool=True
    dpi:Literal[150,300]=150
    items:Annotated[list[Element],Field(min_length=1,max_length=40)]
    @model_validator(mode="after")
    def check(self):
        w,h={"A0":(841,1189),"A1":(594,841),"A2":(420,594),"A3":(297,420),"A4":(210,297)}[self.paper]
        if self.landscape:w,h=h,w
        by={i.id:i for i in self.items}
        if len(by)!=len(self.items) or not 1<=sum(i.kind=="map" for i in self.items)<=4:raise ValueError("Layout needs unique IDs and 1–4 map frames")
        if self.paper not in ("A3","A4") and self.dpi!=150:raise ValueError("Large papers limited to 150 dpi")
        for i in self.items:
            if i.x+i.w>w+.0001 or i.y+i.h>h+.0001:raise ValueError("Element outside paper")
            if (i.camera is not None)!=(i.kind=="map"):raise ValueError("Camera only for maps")
            if i.kind in ("north","scale"):
                if i.map_id not in by or by[i.map_id].kind!="map":raise ValueError("Invalid map link")
            elif i.map_id is not None:raise ValueError("Unexpected map link")
        return self
class LayoutSave(Saved):document:Layout
class LayoutExport(Args):
    layout_id:UUID
    revision:Revision
    format:Literal["pdf","png","json"]="pdf"
class PluginActivate(Args):manifest_id:UUID
class Plugin(Args):extension_key:Key
class PluginRecords(Plugin):entity_name:Key|None=None
class PluginCreate(Plugin):
    entity_name:Key
    payload:dict[str,Any]
    operation_id:UUID
    linked_layer_id:UUID|None=None
    linked_feature_id:Key|None=None
    expected_link_context:dict[str,Any]|None=None
class PluginUpdate(Plugin):
    record_id:UUID
    expected_revision:Revision
    expected_manifest_id:UUID
    payload:dict[str,Any]
    operation_id:UUID
    expected_link_context:dict[str,Any]|None=None
TOOLS={
 "gis_raster_layers":(Empty,"List project raster layers and metadata.",True,False),
 "gis_raster_process":(Raster,"Create a raster result from saved project rasters: NDVI/NDWI/NDBI, band, DEM, clip, RGB, mosaic, calculator, resample etc. Returns operation ticket.",False,False),
 "gis_raster_report":(RasterReport,"Compute zonal statistics or contour from an existing raster. Returns operation ticket.",True,False),
 "gis_raster_download":(RasterId,"Download authorized project GeoTIFF in browser.",False,False),
 "gis_processing_run":(Proc,"Run Buffer, Clip or Dissolve with existing vector processing engine. Creates output layer; returns operation ticket.",False,False),
 "gis_processing_job":(Job,"Read durable vector processing job status and output layer.",True,False),
 "gis_models":(Empty,"List saved Processing Modeler graphs.",True,False),
 "gis_model_save":(ModelSave,"Create or update revision-pinned Processing Modeler graph (up to 12 steps).",False,False),
 "gis_model_run":(ModelRun,"Run saved graph at exact revision, producing independent output layers; returns operation ticket, partial outputs possible.",False,False),
 "gis_layouts":(Empty,"List saved cartography Layout Designer documents.",True,False),
 "gis_layout_save":(LayoutSave,"Create/update revision-pinned print layout: A0–A4, map frames, text, legend, scale, north, grid.",False,False),
 "gis_layout_export":(LayoutExport,"Render and download saved layout at exact revision as PDF, PNG or JSON. Returns operation ticket.",False,False),
 "gis_plugins":(Empty,"List project activations and organization registered extension manifests and entity schemas.",True,False),
 "gis_plugin_activate":(PluginActivate,"Activate an existing registered extension on this project; requires project management permission.",False,False),
 "gis_plugin_disable":(Plugin,"Disable project's active extension; requires project management permission.",False,True),
 "gis_plugin_records":(PluginRecords,"List authorized extension records, optionally by entity.",True,False),
 "gis_plugin_create":(PluginCreate,"Create a record through the active extension schema using idempotent operation UUID.",False,False),
 "gis_plugin_update":(PluginUpdate,"Update extension record with expected revision and manifest, existing schema and permissions.",False,False),
}
LONG={"gis_raster_process":150,"gis_raster_report":150,"gis_raster_download":150,"gis_processing_run":150,"gis_model_run":840,"gis_layout_export":180}
