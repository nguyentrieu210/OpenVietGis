"""Browser acceptance helper; prints only tool result, never pairing secrets."""
import asyncio
import json
import os
from pathlib import Path
import sys
from mcp import Client
from mcp.client.stdio import StdioServerParameters

async def main():
    env = {k: os.environ[k] for k in ("OVG_BRIDGE_BASE", "OVG_BRIDGE_SESSION", "OVG_BRIDGE_TOKEN", "OVG_BRIDGE_DEV")}
    params = StdioServerParameters(command=sys.executable, args=[str(Path(__file__).with_name("live_server.py"))], env=env)
    async with Client(params) as client:
        result = await client.call_tool(sys.argv[1], json.loads(sys.argv[2]))
        print(json.dumps({"is_error": bool(result.is_error), "structured_content": result.structured_content}))

if __name__ == "__main__":
    asyncio.run(main())
