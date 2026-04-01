#!/usr/bin/env python3
"""Generate DPoP-authenticated headers for XRPC requests to a PDS.

Usage:
    ./tools/xrpc-auth.py <METHOD> <URL>

Reads the OAuth session from the active opake account's session.json.
Outputs two lines to stdout (Slumber captures the first via command()):
    Line 1: DPoP <access_token>       (Authorization header value)
    Line 2: <dpop_proof_jwt>          (DPoP header value)

For Slumber, use two separate command() calls — one for each header.
Pass --header=authorization or --header=dpop to select which line to emit.

Requires: pip install cryptography
"""

import base64
import hashlib
import json
import os
import secrets
import sys
import time
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # python < 3.11

from typing import Optional

from cryptography.hazmat.primitives.asymmetric.ec import (
    ECDSA,
    SECP256R1,
    EllipticCurvePrivateKey,
    derive_private_key,
)
from cryptography.hazmat.primitives.hashes import SHA256
from cryptography.hazmat.primitives.asymmetric.utils import decode_dss_signature


def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def b64url_json(obj: dict) -> str:
    return b64url(json.dumps(obj, separators=(",", ":")).encode())


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


def load_session(data_dir: Path, did: str) -> dict:
    session_path = data_dir / "accounts" / sanitize_did(did) / "session.json"
    if not session_path.exists():
        print(f"error: no session at {session_path} — run `opake login` first", file=sys.stderr)
        sys.exit(1)

    session = json.loads(session_path.read_text())

    if session.get("type") == "oauth":
        return session

    print("error: session is not OAuth — DPoP auth requires an OAuth session", file=sys.stderr)
    sys.exit(1)


def strip_query_fragment(url: str) -> str:
    """Strip query and fragment per RFC 9449 §4.2."""
    for sep in ("?", "#"):
        idx = url.find(sep)
        if idx != -1:
            url = url[:idx]
    return url


def create_dpop_proof(
    dpop_key: dict,
    method: str,
    url: str,
    access_token: str,
    dpop_nonce: Optional[str],
) -> str:
    """Create a DPoP proof JWT (ES256, compact JWS)."""
    # Reconstruct the P-256 private key from base64url SEC1 scalar
    private_key_b64 = dpop_key["private_key_b64"]
    # Pad base64url back to standard base64
    padded = private_key_b64 + "=" * (4 - len(private_key_b64) % 4)
    key_bytes = base64.urlsafe_b64decode(padded)
    private_key: EllipticCurvePrivateKey = derive_private_key(
        int.from_bytes(key_bytes, "big"), SECP256R1()
    )

    # Header with embedded JWK
    header = {
        "typ": "dpop+jwt",
        "alg": "ES256",
        "jwk": dpop_key["public_jwk"],
    }

    # jti: random 16 bytes
    jti = b64url(secrets.token_bytes(16))

    # htu: URL without query/fragment
    htu = strip_query_fragment(url)

    # Payload
    payload: dict = {
        "jti": jti,
        "htm": method.upper(),
        "htu": htu,
        "iat": int(time.time()),
    }

    if dpop_nonce:
        payload["nonce"] = dpop_nonce

    # ath: SHA-256 hash of the access token, base64url-encoded
    ath = b64url(hashlib.sha256(access_token.encode()).digest())
    payload["ath"] = ath

    # Encode and sign
    signing_input = f"{b64url_json(header)}.{b64url_json(payload)}"
    der_sig = private_key.sign(signing_input.encode(), ECDSA(SHA256()))

    # Convert DER signature to raw r||s (64 bytes) per JWS ES256 spec
    r, s = decode_dss_signature(der_sig)
    raw_sig = r.to_bytes(32, "big") + s.to_bytes(32, "big")

    return f"{signing_input}.{b64url(raw_sig)}"


def main():
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} [--header=authorization|dpop] <METHOD> <URL>", file=sys.stderr)
        sys.exit(1)

    # Parse optional --header flag
    header_mode = "authorization"
    args = sys.argv[1:]
    if args[0].startswith("--header="):
        header_mode = args[0].split("=", 1)[1].lower()
        args = args[1:]

    if len(args) != 2:
        print(f"usage: {sys.argv[0]} [--header=authorization|dpop] <METHOD> <URL>", file=sys.stderr)
        sys.exit(1)

    method = args[0].upper()
    url = args[1]

    data_dir = opake_data_dir()
    did = resolve_did(data_dir)
    session = load_session(data_dir, did)

    access_token = session["access_token"]
    dpop_key = session["dpop_key"]
    dpop_nonce = session.get("dpop_nonce")

    if header_mode == "authorization":
        print(f"DPoP {access_token}", end="")
    elif header_mode == "dpop":
        proof = create_dpop_proof(dpop_key, method, url, access_token, dpop_nonce)
        print(proof, end="")
    else:
        print(f"error: unknown --header={header_mode}, expected authorization or dpop", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
