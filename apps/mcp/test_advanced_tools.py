import asyncio
import time

import pytest
from live_bridge import BridgeError, BridgeStore
from remote_workspace import OwnedBridge, Owner
from workspace_tools import validate

ORG="00000000-0000-4000-8000-000000000001"
PROJECT="00000000-0000-4000-8000-000000000002"
LAYER="00000000-0000-4000-8000-000000000003"
STEP="00000000-0000-4000-8000-000000000004"
OTHER="00000000-0000-4000-8000-000000000005"

def test_raster_and_layout_model_validation():
    p={"method":"ndvi","name":"NDVI","source_id":LAYER}
    assert validate("gis_raster_process",p)["band_a"]==1
    for x in [p|{"source_id":"../secret"},p|{"method":"mosaic"},p|{"formula":"A+B"},p|{"org_id":ORG}]:
        with pytest.raises(BridgeError):validate("gis_raster_process",x)
    graph={"name":"bad","graph":{"nodes":[{"id":STEP,"algorithm":"band","name":"loop","a":{"type":"step","id":STEP}}]}}
    with pytest.raises(BridgeError):validate("gis_model_save",graph)
    layout={"name":"map","document":{"items":[{"id":STEP,"kind":"map","x":0,"y":0,"w":200,"h":200,"camera":{"lng":106,"lat":11,"zoom":12}}]}}
    assert validate("gis_layout_save",layout)["document"]["paper"]=="A4"
    layout["document"]["items"][0]["w"]=999
    with pytest.raises(BridgeError):validate("gis_layout_save",layout)
    with pytest.raises(BridgeError):validate("gis_plugin_records",{"extension_key":"../admin"})

def test_long_task_tickets_bound_to_subject_and_version():
    store=OwnedBridge(BridgeStore(allow_workspace=True))
    owner=Owner(ORG,"alice",ORG,PROJECT)
    p={"method":"ndvi","name":"NDVI","source_id":LAYER}
    store.create(owner,1)
    with pytest.raises(BridgeError,match="advanced"):
        asyncio.run(store.execute("alice","gis_raster_process",p,time.time()+600))
    session=store.create(owner,2);sid=session["session_id"];bk=session["browser_token"]
    output=asyncio.run(store.execute("alice","gis_raster_process",p,time.time()+600,sid))
    assert output["state"]=="queued" and output["expires_in"]>100
    ticket=output["operation_ticket"]
    assert store.operation_status("alice",ticket,sid)["state"]=="queued"
    store.create(Owner(OTHER,"bob",ORG,PROJECT),2)
    with pytest.raises(BridgeError):store.operation_status("bob",ticket)
    cmd=store.store.poll(sid,bk)["command"];assert cmd["id"]==ticket
    assert store.operation_status("alice",ticket,sid)["state"]=="claimed"
    assert store.store.check_claimed(sid,bk,ticket)["authorized"]
    with pytest.raises(BridgeError):asyncio.run(store.execute("alice","map_view",{},time.time()+600))
    store.store.ack(sid,bk,ticket,True,{"id":LAYER})
    assert store.operation_status("alice",ticket,sid)=={"state":"done","result":{"id":LAYER}}
    assert store.operation_status("alice",ticket,sid)=={"state":"done","result":{"id":LAYER}}
    newer=store.create(owner,2)
    assert store.operation_status("alice",ticket,sid)=={"state":"done","result":{"id":LAYER}}
    with pytest.raises(BridgeError):store.operation_status("alice",ticket,newer["session_id"])

def test_long_expiry_and_denial():
    now=[100.];b=OwnedBridge(BridgeStore(clock=lambda:now[0],allow_workspace=True));s=b.create(Owner(ORG,"alice",ORG,PROJECT),2)
    p={"method":"contour","source_id":LAYER,"interval":2}
    ticket=asyncio.run(b.execute("alice","gis_raster_report",p,time.time()+500))["operation_ticket"]
    b.store.poll(s["session_id"],s["browser_token"])
    b.store.ack(s["session_id"],s["browser_token"],ticket,False,None)
    assert b.operation_status("alice",ticket)=={"state":"denied","result":None}
