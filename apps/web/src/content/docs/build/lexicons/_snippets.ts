// Code snippets for the lexicon reference page.
// Schema-format samples kept readable by rendering as JSON; full
// authoritative schemas live in the repo under /lexicons.

export const wrappedKeyShape = `{
  "did": "did:plc:alice...",         // recipient DID
  "ciphertext": "<bytes>",            // content key, encrypted to their pubkey
  "algo": "x25519-mlkem768-hkdf-a256kw-v2"
}`;

export const encryptionEnvelopeShape = `{
  "algo": "aes-256-gcm",
  "nonce": "<12-byte IV>",
  "keys": [                           // one wrapped copy per recipient
    { "did": "did:plc:alice", "ciphertext": "...", "algo": "x25519-mlkem768-hkdf-a256kw-v2" },
    { "did": "did:plc:bob",   "ciphertext": "...", "algo": "x25519-mlkem768-hkdf-a256kw-v2" }
  ]
}`;

export const keyringRefShape = `{
  "keyring": "at://did:plc:owner/app.opake.keyring/abc123",
  "wrappedContentKey": "<bytes>",     // content key encrypted under group key
  "rotation": 3                       // which generation of the group key
}`;

export const encryptedMetadataShape = `{
  "ciphertext": "<AES-256-GCM ciphertext>",
  "nonce": "<12-byte IV>"
}

// Plaintext (after decryption) is a JSON object:
{
  "name": "budget.pdf",
  "mimeType": "application/pdf",
  "size": 4821,
  "description": "Q4 planning doc",
  "tags": ["finance", "q4"],
  "createdAt": "2026-04-24T10:00:00Z"
}`;

export const documentRecordShape = `{
  "$type": "app.opake.document",
  "opakeVersion": 1,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafyreib..." },
    "mimeType": "application/octet-stream",
    "size": 4821
  },
  "encryption": {
    // Cabinet / direct-share case:
    "$type": "app.opake.document#directEncryption",
    "envelope": { /* encryptionEnvelope, see above */ }

    // ...or workspace case:
    // "$type": "app.opake.document#keyringEncryption",
    // "keyringRef": { /* keyringRef, see above */ },
    // "algo": "aes-256-gcm",
    // "nonce": "<bytes>"
  },
  "encryptedMetadata": { /* see above */ },
  "createdAt": "2026-04-24T10:00:00Z"
}`;

export const grantRecordShape = `{
  "$type": "app.opake.grant",
  "opakeVersion": 1,
  "document": "at://did:plc:alice/app.opake.document/xyz789",
  "recipient": "did:plc:bob",
  "wrappedKey": {
    "did": "did:plc:bob",
    "ciphertext": "<content key, re-wrapped to Bob>",
    "algo": "x25519-mlkem768-hkdf-a256kw-v2"
  },
  "createdAt": "2026-04-24T10:05:00Z"
}`;

export const keyringRecordShape = `{
  "$type": "app.opake.keyring",
  "opakeVersion": 1,
  "members": {
    "did:plc:alice": { "wrappedKey": {...}, "role": "manager" },
    "did:plc:bob":   { "wrappedKey": {...}, "role": "editor" },
    "did:plc:carol": { "wrappedKey": {...}, "role": "viewer" }
  },
  "rotation": 3,
  "keyHistory": {
    "0": { /* prior rotation's members, for historical decryption */ },
    "1": { /* ... */ },
    "2": { /* ... */ }
  },
  "encryptedMetadata": { /* name, description, icon */ },
  "createdAt": "2026-04-20T15:00:00Z"
}`;

export const documentUpdateShape = `{
  "$type": "app.opake.documentUpdate",
  "opakeVersion": 1,
  "target": "at://did:plc:owner/app.opake.document/xyz789",
  "keyring": "at://did:plc:owner/app.opake.keyring/abc123",
  "actionType": "replaceContent",      // or "replaceMetadata"
  "newBlob": { /* blob ref */ },
  "newNonce": "<bytes>",
  "encryptedMetadata": { /* optional */ },
  "createdAt": "2026-04-24T10:10:00Z"
}`;

export const directoryUpdateShape = `{
  "$type": "app.opake.directoryUpdate",
  "opakeVersion": 1,
  "keyring": "at://did:plc:owner/app.opake.keyring/abc123",
  "actionType": "move",                // or "create" / "rename" / "delete" / "placement"
  "target": "at://did:plc:owner/app.opake.directory/old-parent",
  "entry": "at://did:plc:owner/app.opake.document/xyz789",
  "newParent": "at://did:plc:owner/app.opake.directory/new-parent",
  "createdAt": "2026-04-24T10:15:00Z"
}`;
