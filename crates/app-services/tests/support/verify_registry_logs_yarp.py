"""Compare opt-in registry_log_e01 outputs with an explicit, pinned YARP reference."""

import argparse
import hashlib
import importlib.util
import io
import json
import re
import tempfile
from pathlib import Path


REFERENCE_SHA256 = "136c6fd1d5c314ee19c9c6db0d7bd8146eec3256f67b7f31249ddfe630189eb6"


def reference_module(path: Path):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != REFERENCE_SHA256:
        raise ValueError("YARP reference does not match the reviewed source SHA-256")
    spec = importlib.util.spec_from_file_location("registry_log_reference", path)
    if spec is None or spec.loader is None:
        raise ValueError("Cannot load the supplied YARP RegistryFile.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify_original(root: Path, item: dict) -> None:
    original = (root / f"{item['prefix']}.base.bin").read_bytes()
    recovery = item["originalRecovery"]
    digest = hashlib.sha256(original).hexdigest()
    if (
        recovery["status"] != "notNeeded"
        or recovery["baseSha256"] != digest
        or recovery["resultSha256"] != digest
        or recovery["appliedEntries"] != 0
    ):
        raise ValueError("Original clean-hive report disagrees with exported source bytes")
    if item["controlledSimulation"]:
        dirty = (root / f"{item['prefix']}.controlled-dirty.bin").read_bytes()
        if (
            dirty[:4] != original[:4]
            or dirty[12:508] != original[12:508]
            or dirty[512:] != original[512:]
        ):
            raise ValueError("Controlled copy changed bytes outside sequence/checksum fields")


def verify_copy(reference, root: Path, item: dict) -> dict:
    prefix = item["prefix"]
    hive = reference.PrimaryFile(
        io.BytesIO((root / f"{prefix}.controlled-dirty.bin").read_bytes())
    )
    logs = []
    for slot in ("log1", "log2"):
        path = root / f"{prefix}.{slot}.bin"
        if path.exists() and path.stat().st_size:
            logs.append(io.BytesIO(path.read_bytes()))
    if len(logs) == 2:
        hive.apply_new_log_files(*logs)
    elif len(logs) == 1:
        hive.apply_new_log_file(logs[0])
    else:
        raise ValueError("Controlled recovery requires at least one active journal")
    with tempfile.TemporaryDirectory(prefix="yarp-compare-", dir=root) as temporary:
        output = Path(temporary) / "recovered.bin"
        hive.save_recovered_hive(str(output))
        expected = output.read_bytes()
    actual = (root / f"{prefix}.controlled-recovered.bin").read_bytes()
    if actual != expected:
        raise ValueError(f"{prefix}: recovered bytes differ from YARP")
    return {
        "prefix": prefix,
        "matchesAllBytes": True,
        "sha256": hashlib.sha256(actual).hexdigest(),
        "entries": item["controlledSimulation"]["entries"],
        "pages": item["controlledSimulation"]["pages"],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verification-root", type=Path, required=True)
    parser.add_argument("--reference-file", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if args.report.exists():
        raise FileExistsError("Comparison report already exists")
    reference = reference_module(args.reference_file)
    inventory = json.loads((args.verification_root / "verification.json").read_text())
    copies = []
    for item in inventory:
        if re.fullmatch(r"hive-[0-9]+", item["prefix"]) is None:
            raise ValueError("Invalid verification filename prefix")
        verify_original(args.verification_root, item)
        if item["controlledSimulation"]:
            copies.append(verify_copy(reference, args.verification_root, item))
    if not copies:
        raise ValueError("No controlled recovery copies to compare")
    output = {
        "reference": "YARP RegistryFile",
        "referenceSha256": REFERENCE_SHA256,
        "controlledCopies": copies,
        "originalsRemainClean": True,
    }
    with args.report.open("x", encoding="utf-8") as report:
        json.dump(output, report, indent=2)
        report.write("\n")
    print(f"Verified {len(inventory)} clean originals and {len(copies)} byte-identical recoveries")


if __name__ == "__main__":
    main()
