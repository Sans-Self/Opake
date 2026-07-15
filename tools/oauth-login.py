#!/usr/bin/env python3
"""OAuth login for atproto PDS — full browser-based flow.

Two modes:
  Interactive:    ./tools/oauth-login.py
  Slumber hook:   ./tools/oauth-login.py --for-request <METHOD> <URL> <HANDLE>
                  Does OAuth if session expired, then outputs auth headers as JSON:
                  {"authorization": "DPoP ...", "dpop": "<proof>"}

Requires: pip install cryptography
"""

import base64
import hashlib
import http.server
import json
import os
import secrets
import subprocess
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path
from threading import Event
from typing import Optional

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

from cryptography.hazmat.primitives.asymmetric.ec import (
    ECDSA,
    SECP256R1,
    EllipticCurvePrivateKey,
    generate_private_key,
    derive_private_key,
)
from cryptography.hazmat.primitives.hashes import SHA256
from cryptography.hazmat.primitives.asymmetric.utils import decode_dss_signature


# ── Accounts ─────────────────────────────────────────────────────────────────

ACCOUNTS = [
    {"label": "sans-self.org", "handle": "sans-self.org", "bw_id": "5a2ede9f-0dbc-4b7f-ae98-b3ee00511b2f"},
    {"label": "annoiiyed.bsky.social", "handle": "annoiiyed.bsky.social", "bw_id": "7a1e222d-df89-4fa6-aff0-b40c00a6b51c"},
]


# ── Helpers ──────────────────────────────────────────────────────────────────

def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def b64url_json(obj: dict) -> str:
    return b64url(json.dumps(obj, separators=(",", ":")).encode())


def require_https(url: str) -> str:
    # atproto services are always TLS — reject file://, ftp://, etc. before the
    # URL (derived from handle → DID doc → serviceEndpoint) reaches urlopen.
    if urllib.parse.urlparse(url).scheme != "https":
        print(f"error: refusing non-https URL {url!r}", file=sys.stderr)
        sys.exit(1)
    return url


def http_get_json(url: str) -> dict:
    req = urllib.request.Request(require_https(url), headers={"Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as resp:  # noqa: S310 — scheme checked
        return json.loads(resp.read())


def http_post_form(url: str, data: dict, headers: Optional[dict] = None) -> dict:
    body = urllib.parse.urlencode(data).encode()
    req = urllib.request.Request(require_https(url), data=body, method="POST")
    req.add_header("Content-Type", "application/x-www-form-urlencoded")
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:  # noqa: S310 — scheme checked
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        error_body = e.read().decode()
        nonce = e.headers.get("DPoP-Nonce")
        if nonce and "use_dpop_nonce" in error_body:
            raise DPoPNonceRequired(nonce) from e
        print(f"error: HTTP {e.code} from {url}", file=sys.stderr)
        print(f"  {error_body}", file=sys.stderr)
        sys.exit(1)


class DPoPNonceRequired(Exception):
    def __init__(self, nonce: str):
        self.nonce = nonce


# ── Account selection & PDS resolution ───────────────────────────────────────

def select_account() -> dict:
    print("select account:", file=sys.stderr)
    for i, acct in enumerate(ACCOUNTS):
        print(f"  [{i + 1}] {acct['label']}", file=sys.stderr)
    while True:
        try:
            choice = int(input("> ")) - 1
            if 0 <= choice < len(ACCOUNTS):
                return ACCOUNTS[choice]
        except (ValueError, EOFError):
            pass
        print(f"  enter 1-{len(ACCOUNTS)}", file=sys.stderr)


def find_account_by_handle(handle: str) -> dict:
    for acct in ACCOUNTS:
        if acct["handle"] == handle:
            return acct
    print(f"error: unknown handle {handle}", file=sys.stderr)
    sys.exit(1)


def resolve_pds(handle: str) -> str:
    print(f"resolving {handle} ...", file=sys.stderr)
    resp = http_get_json(
        f"https://public.api.bsky.app/xrpc/com.atproto.identity.resolveHandle?handle={urllib.parse.quote(handle)}"
    )
    did = resp["did"]
    print(f"  DID: {did}", file=sys.stderr)

    if did.startswith("did:plc:"):
        doc = http_get_json(f"https://plc.directory/{did}")
    elif did.startswith("did:web:"):
        domain = did.split(":", 2)[2]
        doc = http_get_json(f"https://{domain}/.well-known/did.json")
    else:
        print(f"error: unsupported DID method: {did}", file=sys.stderr)
        sys.exit(1)

    for svc in doc.get("service", []):
        if svc.get("type") == "AtprotoPersonalDataServer":
            pds_url = svc["serviceEndpoint"]
            print(f"  PDS: {pds_url}", file=sys.stderr)
            return pds_url

    print(f"error: no PDS service endpoint for {did}", file=sys.stderr)
    sys.exit(1)


# ── DPoP ─────────────────────────────────────────────────────────────────────

class DPoPKey:
    def __init__(self, private_key: Optional[EllipticCurvePrivateKey] = None):
        if private_key:
            self.private_key = private_key
        else:
            self.private_key = generate_private_key(SECP256R1())
        nums = self.private_key.public_key().public_numbers()
        self.x = b64url(nums.x.to_bytes(32, "big"))
        self.y = b64url(nums.y.to_bytes(32, "big"))

    @classmethod
    def from_session(cls, dpop_key_dict: dict) -> "DPoPKey":
        """Reconstruct from session.json dpop_key field."""
        b64 = dpop_key_dict["private_key_b64"]
        padded = b64 + "=" * (4 - len(b64) % 4)
        key_bytes = base64.urlsafe_b64decode(padded)
        private_key = derive_private_key(int.from_bytes(key_bytes, "big"), SECP256R1())
        return cls(private_key=private_key)

    def public_jwk(self) -> dict:
        return {"kty": "EC", "crv": "P-256", "x": self.x, "y": self.y}

    def proof(self, method: str, url: str, nonce: Optional[str] = None,
              access_token: Optional[str] = None) -> str:
        header = {"typ": "dpop+jwt", "alg": "ES256", "jwk": self.public_jwk()}
        htu = url.split("?")[0].split("#")[0]
        payload = {
            "jti": b64url(secrets.token_bytes(16)),
            "htm": method,
            "htu": htu,
            "iat": int(time.time()),
        }
        if nonce:
            payload["nonce"] = nonce
        if access_token:
            payload["ath"] = b64url(hashlib.sha256(access_token.encode()).digest())

        signing_input = f"{b64url_json(header)}.{b64url_json(payload)}"
        der_sig = self.private_key.sign(signing_input.encode(), ECDSA(SHA256()))
        r, s = decode_dss_signature(der_sig)
        raw_sig = r.to_bytes(32, "big") + s.to_bytes(32, "big")
        return f"{signing_input}.{b64url(raw_sig)}"

    def to_session_dict(self) -> dict:
        private_bytes = self.private_key.private_numbers().private_value.to_bytes(32, "big")
        return {
            "private_key_b64": b64url(private_bytes),
            "public_jwk": self.public_jwk(),
        }


# ── PKCE ─────────────────────────────────────────────────────────────────────

def generate_pkce() -> tuple:
    verifier = b64url(secrets.token_bytes(32))
    challenge = b64url(hashlib.sha256(verifier.encode()).digest())
    return verifier, challenge


# ── Callback server ──────────────────────────────────────────────────────────

class CallbackHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        params = urllib.parse.parse_qs(parsed.query)
        self.server.callback_params = {k: v[0] for k, v in params.items()}
        self.server.callback_received.set()
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.end_headers()
        self.wfile.write(b"<h2>authenticated! you can close this tab.</h2>")

    def log_message(self, format, *args):
        pass


def wait_for_callback(port: int, timeout: int) -> dict:
    server = http.server.HTTPServer(("127.0.0.1", port), CallbackHandler)
    server.callback_params = {}
    server.callback_received = Event()
    server.timeout = timeout
    print(f"  waiting for callback on http://127.0.0.1:{port}/callback ...", file=sys.stderr)
    while not server.callback_received.is_set():
        server.handle_request()
    server.server_close()
    return server.callback_params


# ── Config ───────────────────────────────────────────────────────────────────

def opake_data_dir() -> Path:
    if d := os.environ.get("OPAKE_DATA_DIR"):
        return Path(d)
    if xdg := os.environ.get("XDG_CONFIG_HOME"):
        return Path(xdg) / "opake"
    return Path.home() / ".config" / "opake"


def sanitize_did(did: str) -> str:
    # `did` reaching the write path comes from the token endpoint's `sub`, so a
    # hostile auth server must not be able to escape the accounts directory via
    # path separators or traversal segments.
    if "/" in did or "\\" in did or ".." in did or "\x00" in did:
        print(f"error: refusing unsafe DID {did!r}", file=sys.stderr)
        sys.exit(1)
    return did.replace(":", "_")


# ── Session management ──────────────────────────────────────────────────────

def load_existing_session(handle: str) -> Optional[dict]:
    """Try to find a valid (non-expired) session for the given handle."""
    data_dir = opake_data_dir()
    config_path = data_dir / "config.toml"
    if not config_path.exists():
        return None

    config = tomllib.loads(config_path.read_text())
    for did, entry in config.get("accounts", {}).items():
        if entry.get("handle") == handle:
            session_path = data_dir / "accounts" / sanitize_did(did) / "session.json"
            if session_path.exists():
                session = json.loads(session_path.read_text())
                expires_at = session.get("expires_at", 0)
                if expires_at > time.time() + 30:  # 30s buffer
                    return session
    return None


def do_oauth_login(handle: str) -> dict:
    """Full OAuth flow. Returns the saved session dict."""
    pds_url = resolve_pds(handle)

    port = 22691
    redirect_uri = f"http://127.0.0.1:{port}/callback"
    scope = "atproto transition:generic"
    client_id = f"http://localhost?redirect_uri={urllib.parse.quote(redirect_uri)}&scope={urllib.parse.quote(scope)}"

    dpop = DPoPKey()
    verifier, challenge = generate_pkce()
    state = b64url(secrets.token_bytes(16))

    # Discover AS
    print(f"discovering auth server for {pds_url} ...", file=sys.stderr)
    try:
        rs_meta = http_get_json(f"{pds_url}/.well-known/oauth-protected-resource")
        as_issuer = rs_meta["authorization_servers"][0]
    except Exception:
        as_issuer = pds_url

    as_meta = http_get_json(f"{as_issuer}/.well-known/oauth-authorization-server")
    token_endpoint = as_meta["token_endpoint"]
    authorization_endpoint = as_meta["authorization_endpoint"]
    par_endpoint = as_meta.get("pushed_authorization_request_endpoint", token_endpoint)
    print(f"  AS: {as_issuer}", file=sys.stderr)

    # PAR
    print("pushing authorization request ...", file=sys.stderr)
    par_data = {
        "client_id": client_id, "response_type": "code",
        "redirect_uri": redirect_uri, "scope": scope, "state": state,
        "code_challenge": challenge, "code_challenge_method": "S256",
    }

    dpop_nonce = None
    try:
        par_resp = http_post_form(par_endpoint, par_data, {"DPoP": dpop.proof("POST", par_endpoint)})
    except DPoPNonceRequired as e:
        dpop_nonce = e.nonce
        print(f"  retrying with nonce ...", file=sys.stderr)
        par_resp = http_post_form(par_endpoint, par_data, {"DPoP": dpop.proof("POST", par_endpoint, nonce=dpop_nonce)})

    request_uri = par_resp["request_uri"]
    expires_in = par_resp.get("expires_in", 300)

    # Browser
    auth_url = f"{authorization_endpoint}?client_id={urllib.parse.quote(client_id)}&request_uri={urllib.parse.quote(request_uri)}"
    print(f"opening browser ...", file=sys.stderr)
    subprocess.run(["open", auth_url], check=True)

    # Callback
    params = wait_for_callback(port, expires_in)
    if "error" in params:
        print(f"error: {params.get('error_description', params['error'])}", file=sys.stderr)
        sys.exit(1)
    if params.get("state") != state:
        print("error: state mismatch", file=sys.stderr)
        sys.exit(1)

    # Token exchange
    print("exchanging code for tokens ...", file=sys.stderr)
    token_data = {
        "grant_type": "authorization_code", "client_id": client_id,
        "code": params["code"], "redirect_uri": redirect_uri, "code_verifier": verifier,
    }
    try:
        token_resp = http_post_form(token_endpoint, token_data, {"DPoP": dpop.proof("POST", token_endpoint, nonce=dpop_nonce)})
    except DPoPNonceRequired as e:
        dpop_nonce = e.nonce
        token_resp = http_post_form(token_endpoint, token_data, {"DPoP": dpop.proof("POST", token_endpoint, nonce=dpop_nonce)})

    did = token_resp["sub"]
    print(f"  authenticated as {handle} ({did})", file=sys.stderr)

    # Save
    session = {
        "type": "oauth", "did": did, "handle": handle,
        "access_token": token_resp["access_token"],
        "refresh_token": token_resp.get("refresh_token", ""),
        "dpop_key": dpop.to_session_dict(),
        "token_endpoint": token_endpoint,
        "dpop_nonce": dpop_nonce,
        "expires_at": int(time.time()) + token_resp.get("expires_in", 3600),
        "client_id": client_id,
    }

    data_dir = opake_data_dir()
    account_dir = data_dir / "accounts" / sanitize_did(did)
    account_dir.mkdir(parents=True, exist_ok=True)
    session_path = account_dir / "session.json"
    session_path.write_text(json.dumps(session, indent=2))
    os.chmod(session_path, 0o600)
    print(f"  saved to {session_path}", file=sys.stderr)

    return session


def generate_auth_headers(session: dict, method: str, url: str) -> dict:
    """Generate Authorization + DPoP headers from a session."""
    dpop = DPoPKey.from_session(session["dpop_key"])
    access_token = session["access_token"]
    dpop_nonce = session.get("dpop_nonce")
    proof = dpop.proof(method, url, nonce=dpop_nonce, access_token=access_token)
    return {
        "authorization": f"DPoP {access_token}",
        "dpop": proof,
    }


# ── Entrypoints ─────────────────────────────────────────────────────────────

def main_interactive():
    account = select_account()
    do_oauth_login(account["handle"])
    print("done!", file=sys.stderr)


def main_for_request(method: str, url: str, handle: str):
    """Ensure session is valid (login if needed), output auth headers as JSON."""
    session = load_existing_session(handle)
    if not session:
        print(f"session expired for {handle}, starting OAuth ...", file=sys.stderr)
        session = do_oauth_login(handle)

    headers = generate_auth_headers(session, method, url)
    # JSON to stdout — Slumber picks fields via jq()
    print(json.dumps(headers), end="")


def main():
    if "--for-request" in sys.argv:
        idx = sys.argv.index("--for-request")
        args = sys.argv[idx + 1:]
        if len(args) != 3:
            print(f"usage: {sys.argv[0]} --for-request <METHOD> <URL> <HANDLE>", file=sys.stderr)
            sys.exit(1)
        main_for_request(args[0], args[1], args[2])
    else:
        main_interactive()


if __name__ == "__main__":
    main()
