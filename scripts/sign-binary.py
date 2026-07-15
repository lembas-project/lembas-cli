#!/usr/bin/env python3
"""Sign a binary with an Ed25519 key."""

import sys
from nacl.signing import SigningKey

if len(sys.argv) != 3:
    print(f"Usage: {sys.argv[0]} <binary> <key_file>", file=sys.stderr)
    sys.exit(1)

binary_path = sys.argv[1]
key_path = sys.argv[2]

with open(key_path, "rb") as f:
    signing_key = SigningKey(f.read())

with open(binary_path, "rb") as f:
    binary = f.read()

signature = signing_key.sign(binary).signature

with open(f"{binary_path}.sig", "wb") as f:
    f.write(signature)

print(f"Signed {binary_path}")
