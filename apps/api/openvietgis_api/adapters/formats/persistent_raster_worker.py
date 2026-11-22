"""Blockwise raster calculation and bounded Web Mercator tile pyramid."""
import ast, json, math, sys
from pathlib import Path
from raster_worker import gdal, osr, np, epsg_ref, opened, NODATA
from osgeo import ogr
import raster_worker
raster_worker.PIXELS=20_000_000
WORLD=20037508.342789244
MAX_TILES=192

def read_band(ds,i,x,y,w,h):
    if not 1<=i<=ds.RasterCount: raise ValueError("Band index outside raster")
    b=ds.GetRasterBand(i)
    if gdal.DataTypeIsComplex(b.DataType): raise ValueError("Complex bands unsupported")
    raw=b.ReadAsArray(x,y,w,h).astype(np.float64)
    good=np.isfinite(raw)&(b.GetMaskBand().ReadAsArray(x,y,w,h)!=0)
    nd=b.GetNoDataValue()
    if nd is not None: good &= raw!=nd
    scale,offset=b.GetScale(),b.GetOffset()
    v=raw*(1 if scale is None else scale)+(0 if offset is None else offset)
    return v,good&np.isfinite(v)

def tile_range(bounds,z):
    l,b,r,t=bounds; n=1<<z; step=2*WORLD/n
    clamp=lambda k:max(0,min(n-1,k))
    return clamp(math.floor((l+WORLD)/step)),clamp(math.ceil((r+WORLD)/step)-1),clamp(math.floor((WORLD-t)/step)),clamp(math.ceil((WORLD-b)/step)-1)

def rgba_for(q,method,lo,hi):
    mask=np.isfinite(q)&(q!=NODATA)
    f=np.clip((np.where(mask,q,lo)-lo)/(hi-lo),0,1)
    if method in ("ndvi","ndwi","ndbi","change"):
        ramp=np.array([[165,42,42],[252,242,161],[24,134,65]] if method=="ndvi" else [[193,76,49],[244,244,232],[30,102,174]] if method!="change" else [[185,45,55],[246,245,237],[26,128,75]])
        rgb=np.stack([np.interp(f,[0,.5,1],ramp[:,k]).astype(np.uint8) for k in range(3)],axis=0)
    else: rgb=np.repeat((f*255).astype(np.uint8)[None,:,:],3,axis=0)
    return np.concatenate([rgb,(mask.astype(np.uint8)*255)[None,:,:]],axis=0)

# Parse a deliberately small, array-safe expression language. No eval, attributes,
# indexing, arbitrary calls, imports or exponentiation.
BINARY={ast.Add:np.add,ast.Sub:np.subtract,ast.Mult:np.multiply,ast.Div:np.divide}
COMPARE={ast.Lt:np.less,ast.LtE:np.less_equal,ast.Gt:np.greater,ast.GtE:np.greater_equal,ast.Eq:np.equal,ast.NotEq:np.not_equal}
FUNCS={"abs":(np.abs,1),"sqrt":(np.sqrt,1),"log":(np.log,1),"min":(np.minimum,2),"max":(np.maximum,2)}
def compile_formula(formula):
    if not isinstance(formula,str) or not 1<=len(formula)<=200:raise ValueError("Formula must be 1–200 characters")
    try:tree=ast.parse(formula,mode="eval")
    except SyntaxError as exc:raise ValueError("Invalid formula syntax") from exc
    if sum(1 for _ in ast.walk(tree))>64:raise ValueError("Formula too complex")
    names=set()
    def clean(x):return np.where(np.isfinite(x),x,np.nan)
    def compare(a,b,op):return np.where(np.isfinite(a)&np.isfinite(b),op(a,b).astype(float),np.nan)
    def build(n):
        if isinstance(n,ast.Name) and n.id in ("A","B"):
            names.add(n.id);return lambda env:env[n.id]
        if isinstance(n,ast.Constant) and type(n.value) in (int,float) and math.isfinite(n.value) and abs(n.value)<=1e9:return lambda env:float(n.value)
        if isinstance(n,ast.UnaryOp) and type(n.op) in (ast.UAdd,ast.USub):
            a=build(n.operand);op=np.positive if isinstance(n.op,ast.UAdd) else np.negative;return lambda env:op(a(env))
        if isinstance(n,ast.BinOp) and type(n.op) in BINARY:
            a,b=build(n.left),build(n.right);op=BINARY[type(n.op)];return lambda env:clean(op(a(env),b(env)))
        if isinstance(n,ast.Compare) and len(n.ops)==1 and type(n.ops[0]) in COMPARE:
            a,b=build(n.left),build(n.comparators[0]);op=COMPARE[type(n.ops[0])];return lambda env:compare(a(env),b(env),op)
        if isinstance(n,ast.Call) and isinstance(n.func,ast.Name) and n.func.id in FUNCS and not n.keywords:
            func,arity=FUNCS[n.func.id]
            if len(n.args)==arity:
                args=[build(x) for x in n.args];return lambda env:clean(func(*(fn(env) for fn in args)))
        raise ValueError("Unsupported formula element; use A, B, arithmetic, comparisons, abs, sqrt, log, min, max")
    fn=build(tree.body)
    if "A" not in names:raise ValueError("Formula must reference A")
    return fn,names

def crop_mask(clip,ds,geo,srs):
    if not isinstance(clip,dict) or clip.get("type") not in ("Polygon","MultiPolygon"):raise ValueError("Clip must be WGS84 Polygon/MultiPolygon GeoJSON geometry")
    polygons=[clip.get("coordinates")] if clip["type"]=="Polygon" else clip.get("coordinates")
    if not isinstance(polygons,list) or not polygons:raise ValueError("Invalid polygon")
    count=0;west=180.;east=-180.
    for poly in polygons:
      if not isinstance(poly,list) or not poly:raise ValueError("Invalid polygon")
      for ring in poly:
        if not isinstance(ring,list) or len(ring)<4 or ring[0]!=ring[-1]:raise ValueError("Polygon rings must be closed")
        count+=len(ring)
        if count>5000:raise ValueError("Clip maximum 5000 vertices")
        for pos in ring:
            if not isinstance(pos,list) or len(pos)!=2 or not all(type(v) in (int,float) and math.isfinite(v) for v in pos) or not -180<=pos[0]<=180 or not -85.05112878<=pos[1]<=85.05112878:raise ValueError("Clip coordinates must be WGS84 longitude, latitude")
            west=min(west,pos[0]);east=max(east,pos[0])
    if east-west>180:raise ValueError("Clip must not cross antimeridian")
    geom=ogr.CreateGeometryFromJson(json.dumps(clip))
    if geom is None or geom.IsEmpty() or not geom.IsValid():raise ValueError("Invalid clip polygon")
    transform=osr.CoordinateTransformation(epsg_ref(4326),srs)
    if geom.Transform(transform)!=0 or not geom.IsValid():raise ValueError("Clip reprojection failed")
    xmin,xmax,ymin,ymax=geom.GetEnvelope()
    if not all(math.isfinite(v) for v in (xmin,xmax,ymin,ymax)):raise ValueError("Invalid projected polygon")
    def snap(v):return round(v) if abs(v-round(v))<1e-8 else v
    x0=max(0,math.floor(snap((xmin-geo[0])/geo[1])));x1=min(ds.RasterXSize,math.ceil(snap((xmax-geo[0])/geo[1])))
    y0=max(0,math.floor(snap((ymax-geo[3])/geo[5])));y1=min(ds.RasterYSize,math.ceil(snap((ymin-geo[3])/geo[5])))
    if x1<=x0 or y1<=y0:raise ValueError("Clip does not intersect raster")
    newgeo=(geo[0]+x0*geo[1],geo[1],0,geo[3]+y0*geo[5],0,geo[5])
    mask=gdal.GetDriverByName("MEM").Create("",x1-x0,y1-y0,1,gdal.GDT_Byte);mask.SetGeoTransform(newgeo);mask.SetProjection(srs.ExportToWkt())
    src=ogr.GetDriverByName("Memory").CreateDataSource("");layer=src.CreateLayer("mask",srs,ogr.wkbUnknown)
    feature=ogr.Feature(layer.GetLayerDefn());feature.SetGeometry(geom);layer.CreateFeature(feature)
    if gdal.RasterizeLayer(mask,[1],layer,burn_values=[1])!=0:raise ValueError("Rasterization failed")
    return x0,y0,x1-x0,y1-y0,newgeo,mask

def physical_band(ds,band,path):
    out=gdal.GetDriverByName("GTiff").Create(str(path),ds.RasterXSize,ds.RasterYSize,1,gdal.GDT_Float32,options=["TILED=YES","COMPRESS=DEFLATE"])
    out.SetGeoTransform(ds.GetGeoTransform());out.SetProjection(ds.GetProjection())
    ob=out.GetRasterBand(1);ob.SetNoDataValue(NODATA)
    for y in range(0,ds.RasterYSize,512):
        for x in range(0,ds.RasterXSize,512):
            v,ok=read_band(ds,band,x,y,min(512,ds.RasterXSize-x),min(512,ds.RasterYSize-y))
            ok &= (np.abs(v)<=np.finfo(np.float32).max)&(v!=NODATA)
            ob.WriteArray(np.where(ok,v,NODATA).astype(np.float32),x,y)
    ob=None;out.FlushCache();return out

def preprocess(ds,geo,srs,second,method,a,b,config,root):
    w,h=ds.RasterXSize,ds.RasterYSize
    l,t=geo[0],geo[3];r=l+w*geo[1];bot=t+h*geo[5]
    alg="near"
    if method=="resample":
        factor=config.get("factor");alg=config.get("resampling")
        if type(factor) not in (int,float) or not .1<=factor<=10 or alg not in ("near","bilinear"):raise ValueError("Invalid resampling factor or algorithm")
        w,h=max(1,round(w*factor)),max(1,round(h*factor))
    else:
        if second=="-":raise ValueError("Mosaic requires a second raster")
        ds2,g2,s2,_=opened(second)
        if not srs.IsSame(s2) or not np.allclose([geo[1],geo[5]],[g2[1],g2[5]],rtol=1e-8,atol=1e-12):raise ValueError("Mosaic requires matching CRS and pixel size")
        ox=(g2[0]-l)/geo[1];oy=(g2[3]-t)/geo[5]
        if abs(ox-round(ox))>1e-6 or abs(oy-round(oy))>1e-6:raise ValueError("Mosaic requires aligned pixel origins")
        x0,y0=min(0,round(ox)),min(0,round(oy));x1,y1=max(w,round(ox)+ds2.RasterXSize),max(h,round(oy)+ds2.RasterYSize)
        r,bot=l+x1*geo[1],t+y1*geo[5];l,t=l+x0*geo[1],t+y0*geo[5];w,h=x1-x0,y1-y0
    if w*h>raster_worker.PIXELS:raise ValueError("Result exceeds 20 million pixels")
    sources=[physical_band(ds,a,root/"physical_a.tif")]
    if method=="mosaic":sources.append(physical_band(ds2,b,root/"physical_b.tif"))
    out=gdal.Warp(str(root/"prepared.tif"),sources,format="GTiff",outputBounds=(l,bot,r,t),width=w,height=h,dstSRS=srs.ExportToWkt(),resampleAlg=alg,srcNodata=NODATA,dstNodata=NODATA,creationOptions=["TILED=YES","COMPRESS=DEFLATE"])
    if out is None:raise ValueError("Raster grid processing failed")
    out.FlushCache();out=None;sources.clear()
    return opened(str(root/"prepared.tif"))

def report(ds,geo,srs,code,method,a,config,root):
    if method=="zonal":
        zones=config.get("zones")
        if not isinstance(zones,list) or not 1<=len(zones)<=100:raise ValueError("Provide 1–100 zones")
        result=[];pixels=0
        for zone in zones:
            label=zone.get("label")
            if not isinstance(label,str) or not 1<=len(label)<=160:raise ValueError("Invalid zone label")
            try:ox,oy,w,h,_,mask=crop_mask(zone.get("geometry"),ds,geo,srs)
            except ValueError as exc:
                if str(exc)!="Clip does not intersect raster":raise
                result.append(dict(label=label,valid=0,nodata=0,min=None,max=None,mean=None,std=None,sum=None));continue
            pixels+=w*h
            if pixels>20_000_000:raise ValueError("Zonal report exceeds 20 million cumulative pixels")
            n=nd=0;total=sq=0.;lo=np.inf;hi=-np.inf
            for y in range(0,h,512):
                for x in range(0,w,512):
                    bw,bh=min(512,w-x),min(512,h-y)
                    v,good=read_band(ds,a,ox+x,oy+y,bw,bh);inside=mask.ReadAsArray(x,y,bw,bh)!=0
                    good &= inside;nd+=int(inside.sum()-good.sum());v=v[good];n+=v.size
                    if v.size:total+=float(v.sum());sq+=float(np.square(v).sum());lo=min(lo,float(v.min()));hi=max(hi,float(v.max()))
            mean=total/n if n else None
            result.append(dict(label=label,valid=n,nodata=nd,min=lo if n else None,max=hi if n else None,mean=mean,std=math.sqrt(max(0,sq/n-mean*mean)) if n else None,sum=total if n else None))
        return dict(method=method,zones=result)
    interval=config.get("interval")
    if type(interval) not in (int,float) or not math.isfinite(interval) or interval<=0:raise ValueError("Contour interval must be positive")
    if ds.RasterXSize*ds.RasterYSize>1_000_000:raise ValueError("Contour limited to 1 million pixels; resample first")
    native=physical_band(ds,a,root/"contour.tif");rasterband=native.GetRasterBand(1)
    vals,good=read_band(native,1,0,0,native.RasterXSize,native.RasterYSize)
    if not good.any():raise ValueError("No valid pixels")
    lo,hi=float(vals[good].min()),float(vals[good].max())
    if (hi-lo)/interval>100:raise ValueError("Maximum 100 contour levels; increase interval")
    # A source with extreme absolute values can overflow GDAL contour level indexing.
    if max(abs(lo),abs(hi))/interval>1e8:raise ValueError("Contour elevation/interval ratio too large")
    mem=ogr.GetDriverByName("Memory").CreateDataSource("");layer=mem.CreateLayer("contours",srs,ogr.wkbLineString)
    layer.CreateField(ogr.FieldDefn("elevation",ogr.OFTReal))
    if gdal.ContourGenerate(rasterband,interval,0,[],1,NODATA,layer,-1,0)!=0:raise ValueError("Contour generation failed")
    tr=osr.CoordinateTransformation(srs,epsg_ref(4326));features=[];vertices=0
    for f in layer:
        geom=f.GetGeometryRef()
        if not geom or geom.IsEmpty():continue
        vertices+=geom.GetPointCount()
        if vertices>50000 or len(features)>=10000:raise ValueError("Contour result too complex; increase interval or resample")
        if geom.Transform(tr)!=0:raise ValueError("Contour reprojection failed")
        features.append(dict(type="Feature",geometry=json.loads(geom.ExportToJson()),properties={"elevation":f.GetField("elevation")}))
    return dict(method=method,geojson=dict(type="FeatureCollection",features=features))

def main():
    path,second,method,a,b,out_dir,*extra=sys.argv[1:]; a,b=int(a),int(b)
    root=Path(out_dir)
    ds,geo,srs,code=opened(path); w,h=ds.RasterXSize,ds.RasterYSize
    config=json.loads(Path(extra[0]).read_text()) if extra else {}
    if method in ("zonal","contour"):
        print(json.dumps(report(ds,geo,srs,code,method,a,config,root),allow_nan=False));return
    if method in ("mosaic","resample"):
        ds,geo,srs,code=preprocess(ds,geo,srs,second,method,a,b,config,root);w,h=ds.RasterXSize,ds.RasterYSize;a=1;second="-"
    index=method in ("ndvi","ndwi","ndbi")
    sentinel=config.get("sentinel_baseline")
    if sentinel is not None:
        if not index or ds.RasterCount!=3 or second!="-" or a!=1 or b!=2:raise ValueError("Sentinel crop must contain A, B and SCL bands")
        offset=1000 if float(sentinel)>=4 else 0
    formula,names=compile_formula(config.get("formula")) if method=="calculator" else (None,set())
    rgb=method=="rgb";c=int(config.get("band_c",3));third=config.get("third","-")
    def aligned(p):
        if p=="-":return ds
        other,g2,s2,_=opened(p)
        if (other.RasterXSize,other.RasterYSize)!=(ds.RasterXSize,ds.RasterYSize) or not srs.IsSame(s2) or not np.allclose(geo,g2,rtol=0,atol=1e-8): raise ValueError("Bands require identical grid and CRS")
        return other
    ds2=aligned(second) if method!="calculator" or "B" in names else ds;ds3=aligned(third)
    ox=oy=0;mask_ds=None
    if method=="clip":ox,oy,w,h,geo,mask_ds=crop_mask(config.get("clip"),ds,geo,srs)
    if method not in ("band","ndvi","ndwi","ndbi","hillshade","calculator","change","rgb","clip","slope","aspect","mosaic","resample"):raise ValueError("Unsupported operation")
    output_bands=3 if rgb else 1
    if method in ("hillshade","slope","aspect") and (code==4326 or min(w,h)<3 or abs(srs.GetLinearUnits()-1)>1e-9): raise ValueError("Terrain analysis requires projected metre DEM (at least 3×3)")
    native=root/"native.tif"
    out=gdal.GetDriverByName("GTiff").Create(str(native),w,h,output_bands,gdal.GDT_Float32,options=["TILED=YES","COMPRESS=DEFLATE","PREDICTOR=3"])
    out.SetGeoTransform(geo);out.SetProjection(srs.ExportToWkt())
    for i in range(output_bands):
        ob=out.GetRasterBand(i+1);ob.SetNoDataValue(NODATA)
        if rgb:ob.SetColorInterpretation((gdal.GCI_RedBand,gdal.GCI_GreenBand,gdal.GCI_BlueBand)[i])
    ob=None
    count=0;totals=np.zeros(output_bands);sumsqs=np.zeros(output_bands);lows=np.full(output_bands,np.inf);highs=np.full(output_bands,-np.inf)
    for y in range(0,h,512):
      for x in range(0,w,512):
        bw,bh=min(512,w-x),min(512,h-y)
        if method in ("hillshade","slope","aspect"):
            x0,y0=max(0,x-1),max(0,y-1);x1,y1=min(w,x+bw+1),min(h,y+bh+1)
            v,good=read_band(ds,a,x0,y0,x1-x0,y1-y0)
            dy,dx=np.gradient(v,abs(geo[5]),geo[1]);slope=np.arctan(np.hypot(dx,dy));aspect=np.arctan2(-dx,dy);zen=math.radians(45)
            if method=="slope":v=np.degrees(slope)
            elif method=="aspect":
                v=np.degrees(aspect)%360
            else:v=np.maximum(0,255*(np.cos(zen)*np.cos(slope)+np.sin(zen)*np.sin(slope)*np.cos(math.radians(315)-aspect)))
            mask=good.copy();mask[1:,:]&=good[:-1,:];mask[:-1,:]&=good[1:,:];mask[:,1:]&=good[:,:-1];mask[:,:-1]&=good[:,1:]
            if method=="aspect":mask &= slope>1e-12
            cut=(slice(y-y0,y-y0+bh),slice(x-x0,x-x0+bw));v,good=v[cut],mask[cut]
        else:
            v,good=read_band(ds,a,x+ox,y+oy,bw,bh)
            if index or method=="change" or rgb or "B" in names:
                other,ok=read_band(ds2,b,x,y,bw,bh);good &= ok
            if index:
                if sentinel is not None:
                    scl,ok=read_band(ds,3,x,y,bw,bh)
                    good &= ok&np.isin(scl,[4,5,6])&(v>0)&(other>0)
                    v=np.maximum(0,v-offset);other=np.maximum(0,other-offset)
                den=v+other;good &= np.isfinite(den)&(np.abs(den)>1e-12)
                v=np.divide(v-other,den,out=np.zeros_like(v),where=good)
            elif method=="change":v=other-v  # B = later; A = earlier
            elif method=="calculator":
                with np.errstate(all="ignore"):
                    v=np.broadcast_to(formula({"A":v,**({"B":other} if "B" in names else {})}),v.shape)
            elif rgb:
                blue,ok=read_band(ds3,c,x,y,bw,bh);good &= ok;v=np.stack([v,other,blue])
            elif method=="clip":good &= mask_ds.ReadAsArray(x,y,bw,bh)!=0
        array=v if rgb else v[None,:,:]
        good &= np.all(np.isfinite(array)&(np.abs(array)<=np.finfo(np.float32).max)&(array!=NODATA),axis=0)
        count+=int(good.sum())
        for i in range(output_bands):
            val=np.where(good,array[i],NODATA).astype(np.float32);vals=val[good].astype(np.float64)
            if vals.size:
                totals[i]+=float(vals.sum());sumsqs[i]+=float(np.square(vals).sum());lows[i]=min(lows[i],float(vals.min()));highs[i]=max(highs[i],float(vals.max()))
            out.GetRasterBand(i+1).WriteArray(val,x,y)
    if not count: raise ValueError("No valid pixels")
    out.FlushCache();out=None
    lo,hi=float(lows[0]),float(highs[0])
    source=gdal.Open(str(native),gdal.GA_ReadOnly)
    cog=gdal.GetDriverByName("COG").CreateCopy(str(root/"result.tif"),source,options=["COMPRESS=DEFLATE","PREDICTOR=FLOATING_POINT","BLOCKSIZE=256","OVERVIEWS=AUTO","RESAMPLING=NEAREST","NUM_THREADS=1"])
    cog=None
    ref=epsg_ref(3857);vrt=gdal.AutoCreateWarpedVRT(source,None,ref.ExportToWkt(),gdal.GRA_NearestNeighbour)
    vg=vrt.GetGeoTransform();xmin,ymax=vg[0],vg[3];xmax=xmin+vrt.RasterXSize*vg[1];ymin=ymax+vrt.RasterYSize*vg[5]
    bounds=(xmin,ymin,xmax,ymax)
    if not all(math.isfinite(v) and abs(v)<=WORLD+.001 for v in bounds) or xmin>=xmax or ymin>=ymax: raise ValueError("Extent outside Web Mercator")
    res=max(abs(vg[1]),abs(vg[5]));native_zoom=max(0,min(20,math.ceil(math.log2(2*WORLD/(256*res)))))
    ranges=[];tile_count=0
    for z in range(native_zoom+1):
        x0,x1,y0,y1=tile_range(bounds,z);n=(x1-x0+1)*(y1-y0+1)
        if tile_count+n>MAX_TILES: break
        ranges.append((z,x0,x1,y0,y1));tile_count+=n
    low,high=(-1.,1.) if index else (0.,255.) if method in ("hillshade","rgb") else (-max(abs(lo),abs(hi),1e-9),max(abs(lo),abs(hi),1e-9)) if method=="change" else (0.,90.) if method=="slope" else (0.,360.) if method=="aspect" else (lo,hi)
    if high<=low: high=low+1
    files=[];size=(root/"result.tif").stat().st_size
    if size>64*1024*1024: raise ValueError("Output GeoTIFF exceeds 64 MiB")
    for z,x0,x1,y0,y1 in ranges:
        step=2*WORLD/(1<<z)
        for x in range(x0,x1+1):
            for y in range(y0,y1+1):
                box=(-WORLD+x*step,WORLD-(y+1)*step,-WORLD+(x+1)*step,WORLD-y*step)
                tile=gdal.Warp("",source,format="MEM",dstSRS="EPSG:3857",outputBounds=box,width=256,height=256,resampleAlg="near",srcNodata=NODATA,dstNodata=NODATA)
                q=tile.ReadAsArray()
                if rgb:
                    good=np.all(np.isfinite(q)&(q!=NODATA),axis=0)
                    comps=[]
                    for i in range(3):
                        if highs[i]<=lows[i]:channel=np.full(q[i].shape,128,dtype=np.uint8)
                        else:channel=(np.clip((np.where(good,q[i],lows[i])-lows[i])/(highs[i]-lows[i]),0,1)*255).astype(np.uint8)
                        comps.append(channel)
                    rgba=np.stack([*comps,good.astype(np.uint8)*255])
                else:rgba=rgba_for(q,method,low,high)
                image=gdal.GetDriverByName("MEM").Create("",256,256,4,gdal.GDT_Byte)
                for k,interp in enumerate((gdal.GCI_RedBand,gdal.GCI_GreenBand,gdal.GCI_BlueBand,gdal.GCI_AlphaBand)):
                    pb=image.GetRasterBand(k+1);pb.WriteArray(rgba[k]);pb.SetColorInterpretation(interp)
                pb=None
                name=f"{z}-{x}-{y}.png";png=gdal.GetDriverByName("PNG").CreateCopy(str(root/name),image);png=None
                size+=(root/name).stat().st_size
                if size>128*1024*1024: raise ValueError("Raster and tiles exceed 128 MiB")
                files.append([z,x,y])
    tr=osr.CoordinateTransformation(ref,epsg_ref(4326))
    l,b,_,=tr.TransformPoint(xmin,ymin);r,t,_=tr.TransformPoint(xmax,ymax)
    def stat(i):
        mean=float(totals[i]/count);std=math.sqrt(max(0,float(sumsqs[i]/count)-mean*mean))
        return dict(valid=count,nodata=w*h-count,min=float(lows[i]),max=float(highs[i]),mean=mean,std=std)
    print(json.dumps(dict(width=w,height=h,bands=ds.RasterCount,output_bands=output_bands,epsg=code,method=method,pixel_size=[geo[1],abs(geo[5])],bounds=[l,b,r,t],stats=stat(0),channels=[stat(i) for i in range(output_bands)],legend=[low,high],maxzoom=ranges[-1][0],tiles=files),allow_nan=False))

if __name__=="__main__":
    try: main()
    except (ValueError,RuntimeError,OverflowError) as exc:
        print(json.dumps({"error":str(exc)[:280]}));sys.exit(2)
