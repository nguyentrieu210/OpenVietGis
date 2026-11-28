"""Standalone golden GDAL fixture; no live database or service needed."""
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

import numpy as np
from osgeo import gdal

worker=pathlib.Path(__file__).resolve().parents[2]/"apps/api/openvietgis_api/adapters/formats/georeference_worker.py"

def test():
    with tempfile.TemporaryDirectory() as tmp:
        root=pathlib.Path(tmp); src=root/"source.tif"; out=root/"result.tif"; cfg=root/"config.json"
        pixels=np.arange(100,dtype=np.uint16).reshape(10,10)
        ds=gdal.GetDriverByName("GTiff").Create(str(src),10,10,1,gdal.GDT_UInt16)
        ds.GetRasterBand(1).WriteArray(pixels);ds.GetRasterBand(1).SetNoDataValue(65535);ds=None
        def run(body,success=True):
            cfg.write_text(json.dumps(body))
            p=subprocess.run([sys.executable,str(worker),str(src),str(cfg),str(out)],capture_output=True,text=True,timeout=30,check=False)
            result=json.loads(p.stdout)
            assert (p.returncode==0)==success,(p.stdout,p.stderr)
            return result
        inspect=run({"mode":"inspect"})
        assert inspect["width"]==10 and inspect["source_sha256"]==hashlib.sha256(src.read_bytes()).hexdigest()
        base={"mode":"solve","epsg":3857,"source_sha256":inspect["source_sha256"],"points":[[0,0,1000,2000],[10,0,1020,2005],[0,10,1003,1970],[10,10,1023,1975]]}
        r=run(base)
        assert np.allclose(r["transform"],[1000,2,.3,2000,.5,-3],atol=1e-9)
        assert r["rms"]<1e-9 and r["redundant"]
        ds=gdal.Open(str(out));assert ds.GetSpatialRef().GetAuthorityCode(None)=="3857"
        assert np.array_equal(ds.GetRasterBand(1).ReadAsArray(),pixels)
        assert ds.GetRasterBand(1).GetNoDataValue()==65535;ds=None
        run(base|{"points":[[0,0,0,0],[1,1,1,1],[2,2,2,2]]},False)
        run(base|{"source_sha256":"0"*64},False)
        print("PASS affine GCP golden transform, original pixels/NoData, collinear and digest rejection")
if __name__=="__main__":test()
