# app.opake.cloud.* Lexicon Schemas

An encrypted personal cloud built on AT Protocol.

## Architecture

The encryption model follows the same hybrid pattern as git-crypt:
- Each file/record is encrypted with a **random symmetric key** (AES-256-GCM)
- That symmetric key is **wrapped** (encrypted) to each authorized DID's public key
- Wrapped keys are stored as atproto records, publicly visible but useless without the private key
- File content is uploaded as a PDS blob (opaque encrypted bytes)

## Lexicon Overview

| NSID | Type | Purpose |
|------|------|---------|
| `app.opake.cloud.defs` | defs | Shared type definitions (encryption envelope, wrapped key, etc.) |
| `app.opake.cloud.document` | record | An encrypted file/document with metadata |
| `app.opake.cloud.keyring` | record | A named group with a shared symmetric key, wrapped to each member |
| `app.opake.cloud.grant` | record | A share grant — gives a DID access to a specific document's key |

## Flow: Sharing a file with another DID

```
1. Alice creates a document:
   - Generates random AES-256-GCM key K
   - Encrypts file content with K → uploads as blob
   - Wraps K to her own DID pubkey → stores in document record

2. Alice shares with Bob (did:plc:bob):
   - Resolves did:plc:bob → gets public key from DID document
   - Wraps K to Bob's pubkey
   - Creates a grant record pointing to the document, containing Bob's wrapped key

3. Bob's client:
   - Discovers grant record (via AppView query, or notification)
   - Fetches the referenced document record
   - Finds his wrapped key in the grant
   - Decrypts K with his private key
   - Fetches the encrypted blob via com.atproto.sync.getBlob
   - Decrypts blob with K
```

## Flow: Group sharing via keyring

```
1. Alice creates a keyring "family-photos":
   - Generates group symmetric key GK
   - Wraps GK to each member's DID pubkey
   - Stores as keyring record

2. Alice creates documents referencing the keyring:
   - Each document's content key is encrypted with GK (not individual pubkeys)
   - Any keyring member can derive K from GK

3. Adding a new member:
   - Alice wraps GK to the new member's pubkey
   - Updates the keyring record
   - New member can now decrypt all documents in the group — no per-document re-encryption needed
```
