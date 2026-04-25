#!/usr/bin/env python3
"""Generate an Opake-Ed25519 Authorization header for the indexer.

Usage:
    ./tools/indexer-auth.py <METHOD> <PATH>

Reads the signing key from the active opake identity. The DID is taken from
the opake config (default_did), or overridden via OPAKE_DID env var.

Outputs the full header value to stdout, ready for Slumber's command() template.

Requires: pip install cryptography
"""

import base64
import json
import os
import sys
import time
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # python < 3.11

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


def opake_data_dir() -> Path:
    if d := os.environ.get("OPAKE_DATA_DIR"):
        return Path(d)
    if xdg := os.environ.get("XDG_CONFIG_HOME"):
        return Path(xdg) / "opake"
    return Path.home() / ".config" / "opake"


def sanitize_did(did: str) -> str:
    return did.replace(":", "_")


def resolve_did(data_dir: Path) -> str:
    if did := os.environ.get("OPAKE_DID"):
        return did

    config_path = data_dir / "config.toml"
    if not config_path.exists():
        print(f"error: no config at {config_path} — set OPAKE_DID", file=sys.stderr)
        sys.exit(1)

    config = tomllib.loads(config_path.read_text())
    did = config.get("default_did")
    if not did:
        print("error: no default_did in config — set OPAKE_DID", file=sys.stderr)
        sys.exit(1)
    return did


def load_signing_key(data_dir: Path, did: str) -> Ed25519PrivateKey:
    identity_path = data_dir / "accounts" / sanitize_did(did) / "identity.json"
    if not identity_path.exists():
        print(f"error: no identity at {identity_path}", file=sys.stderr)
        sys.exit(1)

    identity = json.loads(identity_path.read_text())
    signing_key_b64 = identity.get("signing_key")
    if not signing_key_b64:
        print("error: identity has no signing_key — run `opake recover` to upgrade", file=sys.stderr)
        sys.exit(1)

    key_bytes = base64.b64decode(signing_key_b64)
    return Ed25519PrivateKey.from_private_bytes(key_bytes)


def sign_request(method: str, path: str, did: str, key: Ed25519PrivateKey) -> str:
    timestamp = int(time.time())
    message = f"{method}:{path}:{timestamp}:{did}"
    signature = key.sign(message.encode())
    sig_b64 = base64.b64encode(signature).decode()
    return f"Opake-Ed25519 {did}:{timestamp}:{sig_b64}"


def main():
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <METHOD> <PATH>", file=sys.stderr)
        sys.exit(1)

    method = sys.argv[1].upper()
    path = sys.argv[2]

    data_dir = opake_data_dir()
    did = resolve_did(data_dir)
    key = load_signing_key(data_dir, did)
    header = sign_request(method, path, did, key)

    # stdout only — slumber captures this
    print(header, end="")


if __name__ == "__main__":
    main()
