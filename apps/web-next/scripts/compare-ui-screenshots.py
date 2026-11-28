"""Compare two explicitly supplied screenshot directories. Requires Pillow."""
import argparse
import json
from pathlib import Path
from PIL import Image, ImageChops

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--max-changed-ratio", type=float, default=0.001)
    parser.add_argument("--channel-threshold", type=int, default=8)
    args = parser.parse_args()
    if not 0 <= args.max_changed_ratio <= 1 or not 0 <= args.channel_threshold <= 255:
        parser.error("thresholds out of range")
    expected = {p.name for p in args.baseline.glob("*.png")}
    actual = {p.name for p in args.candidate.glob("*.png")}
    if not expected or expected != actual:
        print(json.dumps({"missing": sorted(expected - actual), "extra": sorted(actual - expected), "baseline_count": len(expected)}))
        return 1
    failed = False
    for name in sorted(expected):
        with Image.open(args.baseline / name) as before, Image.open(args.candidate / name) as after:
            if before.size != after.size:
                print(json.dumps({"file": name, "status": "size mismatch", "baseline": before.size, "candidate": after.size}))
                failed = True
                continue
            diff = ImageChops.difference(before.convert("RGB"), after.convert("RGB"))
            channels = diff.split()
            largest = ImageChops.lighter(ImageChops.lighter(channels[0], channels[1]), channels[2])
            histogram = largest.histogram()
            ratio = sum(histogram[args.channel_threshold + 1:]) / (before.width * before.height)
            passed = ratio <= args.max_changed_ratio
            failed |= not passed
            print(json.dumps({"file": name, "changed_ratio": ratio, "pass": passed}))
    return int(failed)

if __name__ == "__main__":
    raise SystemExit(main())
