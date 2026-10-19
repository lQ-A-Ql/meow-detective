"""Build the single signed portable EXE and its static update index (no installers)."""
import argparse
import base64
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MAGIC = b"MEOW-PORTABLE-V1"


def schema_digest(root=ROOT):
    digest = hashlib.sha256()
    for path in sorted((root / "crates/persistence-sqlite/src/migrations/scripts").glob("*.sql")):
        digest.update(path.name.encode() + b"\0")
        digest.update(path.read_text(encoding="utf-8").replace("\r\n", "\n").encode() + b"\0")
    return digest.hexdigest()


def version_for_tag(tag, root=ROOT):
    if not re.fullmatch(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", tag):
        raise ValueError("release tags must be stable vMAJOR.MINOR.PATCH")
    version = tag[1:]
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    frontend = json.loads((root / "frontend/package.json").read_text(encoding="utf-8"))
    tauri = json.loads((root / "apps/desktop/src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    if any(item != version for item in [cargo["workspace"]["package"]["version"], frontend["version"], tauri["version"]]):
        raise ValueError("tag, workspace, frontend and Tauri versions must agree")
    return version


def check_public_key(key):
    lines = base64.b64decode(key.strip(), validate=True).decode().splitlines()
    if len(lines) != 2 or not lines[0].startswith("untrusted comment:"):
        raise ValueError("TAURI_UPDATER_PUBLIC_KEY must contain the Tauri public key, not its path")
    decoded = base64.b64decode(lines[1], validate=True)
    if len(decoded) != 42 or decoded[:2] != b"Ed":
        raise ValueError("invalid minisign public key")


def package(binary, output, version):
    if binary.read_bytes()[:2] != b"MZ":
        raise ValueError("expected a Windows PE executable")
    output.mkdir(parents=True, exist_ok=True)
    destination = output / f"meow-detective_v{version}_windows-x86_64.exe"
    if destination.exists():
        raise FileExistsError(destination)
    metadata = json.dumps({"format": 1, "version": version, "caseSchema": schema_digest()}, separators=(",", ":")).encode()
    shutil.copyfile(binary, destination)
    with destination.open("ab") as stream:
        stream.write(metadata + struct.pack("<I", len(metadata)) + MAGIC)
        stream.flush()
        os.fsync(stream.fileno())
    return destination


def manifest(binary, version, notes):
    signature = binary.with_suffix(binary.suffix + ".sig").read_text(encoding="utf-8").strip()
    # Actual signer output is required. The Rust verifier validates it against the EXE before upload.
    decoded = base64.b64decode(signature, validate=True).decode()
    if not decoded.startswith("untrusted comment:") or "trusted comment:" not in decoded:
        raise ValueError("invalid Tauri signature file")
    document = {
        "version": version, "notes": notes,
        "pub_date": dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
        "package_format": 1, "case_schema": schema_digest(),
        "platforms": {"windows-x86_64": {
            "signature": signature,
            "url": f"https://github.com/lQ-A-Ql/meow-detective/releases/download/v{version}/{binary.name}",
        }},
    }
    destination = binary.parent / "latest.json"
    if destination.exists():
        raise FileExistsError(destination)
    destination.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["preflight", "package", "manifest"])
    parser.add_argument("--tag", required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/portable-release")
    parser.add_argument("--notes", type=Path)
    arguments = parser.parse_args()
    version = version_for_tag(arguments.tag)
    if arguments.mode == "preflight":
        check_public_key(os.environ.get("MEOW_UPDATER_PUBLIC_KEY", ""))
        print(f"release {version}; case schema {schema_digest()}")
    elif arguments.mode == "package":
        print(package(arguments.binary, arguments.output, version))
    else:
        print(manifest(arguments.binary, version, arguments.notes.read_text(encoding="utf-8") if arguments.notes else ""))


if __name__ == "__main__":
    main()
