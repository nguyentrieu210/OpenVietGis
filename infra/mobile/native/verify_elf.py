"""Reject malformed, wrong-ABI or incompatible PT_LOAD alignment (not runtime proof)."""
import argparse
import json
import struct
from pathlib import Path

def inspect(data):
    if len(data) < 64 or data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("expected little-endian ELF64")
    hdr = struct.unpack_from("<16sHHIQQQIHHHHHH", data)
    if hdr[2] != 183:
        raise ValueError("expected AArch64")
    phoff, entsize, count = hdr[5], hdr[9], hdr[10]
    if entsize != 56 or count == 0 or phoff + entsize * count > len(data):
        raise ValueError("invalid program headers")
    loads = []
    for i in range(count):
        typ, flags, offset, va, pa, fsize, msize, align = struct.unpack_from("<IIQQQQQQ", data, phoff + i * entsize)
        if typ != 1:
            continue
        if align < 16384 or align & (align - 1) or (va - offset) % align or offset + fsize > len(data) or fsize > msize:
            raise ValueError("incompatible PT_LOAD")
        loads.append({"offset": offset, "vaddr": va, "alignment": align})
    if not loads:
        raise ValueError("no loadable segments")
    return loads

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    result = [{"file": p.name, "loads": inspect(p.read_bytes())} for p in args.files]
    print(json.dumps(result, indent=2))

if __name__ == "__main__":
    main()
