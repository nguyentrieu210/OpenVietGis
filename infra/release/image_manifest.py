"""Validate immutable image receipts and generate an inert Compose override.

No registry or Docker changes are made. A receipt identifies a build, not a
security review, release approval, or database migration authorization.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROLES = ("api", "web", "migrate", "postgres")
SHA = re.compile(r"[0-9a-f]{40}")
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")
REPO = re.compile(r"[a-z0-9][a-z0-9_.-]*/[a-z0-9][a-z0-9_.-]*")


def validate(manifest: dict, repository: str, candidate: str) -> dict[str, str]:
    if not REPO.fullmatch(repository) or not SHA.fullmatch(candidate):
        raise ValueError("Invalid trusted repository or candidate SHA")
    if not isinstance(manifest, dict) or set(manifest) != {
        "schema", "source_sha", "repository", "platform", "images"
    }:
        raise ValueError("Invalid manifest fields")
    if (type(manifest["schema"]) is not int or manifest["schema"] != 1 or manifest["source_sha"] != candidate
            or manifest["repository"] != repository or manifest["platform"] != "linux/amd64"):
        raise ValueError("Unexpected candidate, repository or platform")
    images = manifest["images"]
    if not isinstance(images, dict) or set(images) != set(ROLES):
        raise ValueError("Expected exactly four image roles")
    result = {}
    for role in ROLES:
        row = images[role]
        if not isinstance(row, dict) or set(row) != {"ref", "source_sha"}:
            raise ValueError(f"Invalid receipt for {role}")
        prefix = f"ghcr.io/{repository}-{role}@"
        ref = row["ref"]
        if (row["source_sha"] != candidate or not isinstance(ref, str)
                or not ref.startswith(prefix) or not DIGEST.fullmatch(ref[len(prefix):])):
            raise ValueError(f"Untrusted or mutable image reference for {role}")
        result[role] = ref
    if len({ref.split("@")[1] for ref in result.values()}) != len(ROLES):
        raise ValueError("Duplicate role digests")
    return result


def compose_override(images: dict[str, str]) -> str:
    # Compose >=2.24.4 supports !reset. Do not replace networking, env,
    # ports, database volumes, or the existing migration command.
    lines = ["# Generated digest-only candidate; not release approval.", "services:"]
    for role in ROLES:
        lines += [f"  {role}:", "    build: !reset null", f"    image: {images[role]}",
                  "    pull_policy: never"]
    return "\n".join(lines) + "\n"


def verify_compose(config: dict, images: dict[str, str]) -> None:
    services = config.get("services") if isinstance(config, dict) else None
    if not isinstance(services, dict) or not set(ROLES).issubset(services):
        raise ValueError("Expected Rust-live service roles in resolved Compose")
    if "api-rust" in services:
        raise ValueError("Hybrid fixture is not the Rust-live deployment profile")
    for role, service in services.items():
        if not isinstance(service, dict) or service.get("build"):
            raise ValueError(f"Local build is still enabled for {role}")
        if role in images and (service.get("image") != images[role]
                               or service.get("pull_policy") != "never"):
            raise ValueError(f"Resolved Compose does not pin {role}")


def verify_local(images: dict[str, str], candidate: str) -> None:
    for role, ref in images.items():
        # Inspect only locally pulled digest references; no network or mutation.
        process = subprocess.run(
            ["docker", "image", "inspect", ref], capture_output=True, text=True, check=False
        )
        if process.returncode:
            raise ValueError(f"Required digest not present locally: {role}")
        info = json.loads(process.stdout)
        if not isinstance(info, list) or len(info) != 1:
            raise ValueError(f"Unexpected local image result: {role}")
        item = info[0]
        labels = (item.get("Config") or {}).get("Labels") or {}
        if (ref not in (item.get("RepoDigests") or [])
                or item.get("Os") != "linux" or item.get("Architecture") != "amd64"
                or labels.get("org.opencontainers.image.revision") != candidate):
            raise ValueError(f"Digest, architecture or revision mismatch: {role}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--repository", required=True, help="Trusted lowercase owner/repo")
    parser.add_argument("--candidate", required=True, help="Independently approved source SHA")
    parser.add_argument("--format", choices=("compose", "refs", "verify-compose", "verify-local"), default="compose")
    args = parser.parse_args()
    try:
        images = validate(json.loads(args.manifest.read_text()), args.repository, args.candidate)
        if args.format == "compose":
            print(compose_override(images), end="")
        elif args.format == "refs":
            print("\n".join(images.values()))
        elif args.format == "verify-compose":
            verify_compose(json.load(sys.stdin), images)
            print("Resolved Compose digest pins verified; no builds enabled.")
        else:
            verify_local(images, args.candidate)
            print("Local image digest, platform and revision labels verified.")
    except (ValueError, OSError, TypeError) as error:
        print(f"Invalid image receipt: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
