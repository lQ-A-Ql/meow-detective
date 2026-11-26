from pathlib import Path
import hashlib
import random
import sys


root = Path(sys.argv[1]).resolve()
payload = random.Random(20261126).randbytes(262144)
root.mkdir(parents=True, exist_ok=True)
(root / "payload.bin").write_bytes(payload)
(root / "payload.sha256").write_text(hashlib.sha256(payload).hexdigest() + "\n")
