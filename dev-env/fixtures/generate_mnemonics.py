#!/usr/bin/env python3
"""Generate the dev-env fixture mnemonics (BIP-39, 24 words).

Run once; the output is checked in as actors.json. These phrases are PUBLIC
TEST VECTORS for the hermetic dev-env — they are not secrets, must never be
used on a live PDS, and are deliberately kept outside git-crypt so nobody
mistakes them for real credentials (contrast tests/accounts.secret).

Regenerating changes every fixture identity: the dev-env spec guarantees
stable encryption keys across resets *for a given phrase*, so treat the
checked-in actors.json as canonical and rerun this only when deliberately
rotating the whole fixture set.

Wordlist: crates/opake-crypto/src/bip39_english.txt (the same list the
product derives from). Checksum per BIP-39: 256 bits entropy + first 8 bits
of SHA-256(entropy) = 264 bits = 24 x 11-bit indices.
"""

import hashlib
import json
import secrets
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
WORDS = (REPO / "crates/opake-crypto/src/bip39_english.txt").read_text().split()
assert len(WORDS) == 2048

ACTORS = [
    ("alice", "pds-a"),
    ("bob", "pds-a"),
    ("carol", "pds-b"),
    ("dave", "pds-b"),
    ("eve", "pds-c"),
    ("frank", "pds-c"),
]


def mnemonic() -> str:
    entropy = secrets.token_bytes(32)
    checksum = hashlib.sha256(entropy).digest()[0]
    bits = int.from_bytes(entropy, "big") << 8 | checksum
    indices = [(bits >> (11 * i)) & 0x7FF for i in range(23, -1, -1)]
    return " ".join(WORDS[i] for i in indices)


def main() -> None:
    actors = [
        {
            "name": name,
            "handle": f"{name}.{pds}.test",
            "pds": pds,
            "mnemonic": mnemonic(),
        }
        for name, pds in ACTORS
    ]
    out = Path(__file__).with_name("actors.json")
    out.write_text(
        json.dumps(
            {
                "_comment": "PUBLIC TEST VECTORS for the hermetic dev-env. "
                "Not secrets. Never valid on a live PDS. See generate_mnemonics.py.",
                "actors": actors,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"wrote {out} ({len(actors)} actors)")


if __name__ == "__main__":
    main()
