"""Host-only vector format screening. Synthetic XY points, no mobile claims."""
import argparse,json,os,resource,sys,tempfile,time,subprocess
from pathlib import Path

def worker(n,fmt,path):
 import numpy as np, geopandas as gpd, pyogrio
 x=106.0+(np.arange(n)%1000)*0.0001;y=10.0+(np.arange(n)//1000)*0.0001
 t=time.perf_counter();g=gpd.GeoDataFrame({'id':np.arange(n),'name':['sample']*n},geometry=gpd.points_from_xy(x,y),crs='EPSG:4326');gen=time.perf_counter()-t
 t=time.perf_counter();pyogrio.write_dataframe(g,path,driver=fmt);write=time.perf_counter()-t
 del g
 t=time.perf_counter();out=pyogrio.read_dataframe(path);read=time.perf_counter()-t
 assert len(out)==n and set(out.id)==set(range(n)) and out.crs.to_epsg()==4326
 assert abs(float(out.loc[out.id==n-1].geometry.x.iloc[0])-x[-1])<1e-9
 t=time.perf_counter();p=pyogrio.read_dataframe(path,bbox=(106.0,10.0,106.01,10.01));bbox=time.perf_counter()-t
 assert len(p)>0
 return {'rows':n,'driver':fmt,'bytes':os.stat(path).st_size,'generate_s':round(gen,4),'write_s':round(write,4),'read_s':round(read,4),'bbox_s':round(bbox,4),'bbox_rows':len(p),'peak_rss_kib':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,'gdal':pyogrio.__gdal_version__,'pyogrio':pyogrio.__version__,'python':sys.version.split()[0]}
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--worker',action='store_true');p.add_argument('--rows',type=int);p.add_argument('--format');p.add_argument('--path');a=p.parse_args()
 if a.worker: print(json.dumps(worker(a.rows,a.format,a.path)))
 else:
  results=[]
  with tempfile.TemporaryDirectory() as d:
   for n in (10000,100000):
    for fmt,ext in [('GeoJSON','geojson'),('GPKG','gpkg'),('FlatGeobuf','fgb')]:
     out=subprocess.check_output([sys.executable,__file__,'--worker','--rows',str(n),'--format',fmt,'--path',str(Path(d)/f'{n}.{ext}')],text=True)
     results.append(json.loads(out));print(out.strip(),flush=True)
  print(json.dumps({'results':results,'scope':'host synthetic screening; single process per format; RSS includes imports; no mobile, columnar or copy count proof'},indent=2))
