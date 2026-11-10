"""Verify downloaded inputs against GitHub's SHA-256 release asset digests."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

tag, folder = sys.argv[1:]
assets = json.loads(subprocess.check_output(
    ["gh", "release", "view", tag, "--json", "assets"], text=True
))["assets"]
digests = {a["name"]: a["digest"] for a in assets}
files = list(Path(folder).iterdir())
if not files:
    raise SystemExit("No release inputs downloaded")
for file in files:
    actual = "sha256:" + hashlib.sha256(file.read_bytes()).hexdigest()
    if actual != digests.get(file.name):
        raise SystemExit(f"Checksum mismatch: {file.name}")
    print(f"Verified {file.name}")
