<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the AT Protocol 
  schemas or lexicon definitions in this file, you MUST also update the 
  corresponding MDX content in `apps/web/src/content/` to prevent documentation drift. 
-->

# at.opake.* Lexicon Schemas

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
| `at.opake.accountConfig` | record | Singleton per-account config/preferences (rkey: `self`), synced across devices |
| `at.opake.defs` | defs | Shared type definitions (encryption envelope, wrapped key, etc.) |
| `at.opake.directory` | record | A directory containing an ordered list of child document/directory AT-URIs |
| `at.opake.document` | record | An encrypted file/document with metadata |
| `at.opake.publicKey` | record | Singleton X25519 encryption public key (rkey: `self`) for key discovery |
| `at.opake.keyring` | record | A named group (workspace) with a shared symmetric key, wrapped to each member with a role |
| `at.opake.grant` | record | A share grant — gives a DID access to a specific document's key |
| `at.opake.documentUpdate` | record | A proposed update to another member's document — content, metadata, or adoption |
| `at.opake.directoryUpdate` | record | A proposed structural change to a workspace directory (placement, move, create, rename, delete) |
| `at.opake.pendingShare` | record | A queued share intent — retried by daemon until recipient signs up or expires (7 days) |
| `at.opake.keyringUpdate` | record | A proposed update to a workspace keyring (member add/remove, metadata, role change) |
| `at.opake.pairRequest` | record | Ephemeral public key from a new device requesting identity transfer |
| `at.opake.pairResponse` | record | Encrypted identity payload sent in response to a pair request |
| `at.opake.authFullAccess` | permission-set | OAuth permission set bundling all `at.opake.*` collections — for `include:` scopes |

## Flow: Sharing a file with another DID

```mermaid
sequenceDiagram
    participant Alice
    participant AlicePDS as Alice's PDS
    participant PLC as PLC Directory
    participant BobPDS as Bob's PDS
    participant Bob

    Note over Alice,AlicePDS: 1. Alice creates an encrypted document
    Alice->>Alice: Generate random AES-256-GCM key K
    Alice->>Alice: Encrypt file with K → ciphertext
    Alice->>AlicePDS: uploadBlob(ciphertext)
    Alice->>Alice: Wrap K to own pubkey
    Alice->>AlicePDS: createRecord(document)

    Note over Alice,BobPDS: 2. Alice shares with Bob
    Alice->>PLC: Resolve did:plc:bob
    PLC-->>Alice: Bob's DID document → PDS URL
    Alice->>BobPDS: getRecord(publicKey/self)
    BobPDS-->>Alice: Bob's X25519 public key
    Alice->>Alice: Wrap K to Bob's pubkey
    Alice->>AlicePDS: createRecord(grant)

    Note over Bob,AlicePDS: 3. Bob downloads the shared file
    Bob->>AlicePDS: getRecord(grant)
    AlicePDS-->>Bob: Grant record (wrappedKey)
    Bob->>Bob: Unwrap K with private key
    Bob->>AlicePDS: getRecord(document)
    AlicePDS-->>Bob: Document record (nonce, blob ref)
    Bob->>AlicePDS: getBlob(cid)
    AlicePDS-->>Bob: Encrypted blob
    Bob->>Bob: Decrypt blob with K + nonce → plaintext
```

Data never leaves Alice's PDS. Bob fetches everything from the source.

## Flow: Group sharing via keyring

```mermaid
sequenceDiagram
    participant Alice
    participant PDS as Alice's PDS

    Note over Alice,PDS: 1. Create keyring
    Alice->>Alice: Generate group key GK
    Alice->>Alice: Wrap GK to Alice, Bob, Carol
    Alice->>PDS: createRecord(keyring)

    Note over Alice,PDS: 2. Upload document under keyring
    Alice->>Alice: Generate content key K, encrypt file
    Alice->>PDS: uploadBlob(ciphertext)
    Alice->>Alice: Wrap K under GK (symmetric)
    Alice->>PDS: createRecord(document, keyringRef)

    Note over Alice,PDS: 3. Add new member (Dave)
    Alice->>Alice: Wrap GK to Dave's pubkey
    Alice->>PDS: updateRecord(keyring, add Dave)
    Note right of PDS: Dave can now decrypt all<br/>documents under this keyring
```

Any keyring member unwraps GK with their private key, then uses GK to unwrap each document's content key K. Removing a member archives the old rotation's member entries into `keyHistory`, then rotates GK and re-wraps to the remaining members — per-document content keys and blobs stay untouched. The history lets remaining members decrypt pre-rotation documents even on new devices.

## Flow: Collaborative editing via documentUpdate

```mermaid
sequenceDiagram
    participant Editor
    participant EditorPDS as Editor's PDS
    participant Indexer
    participant Owner
    participant OwnerPDS as Owner's PDS

    Note over Editor,EditorPDS: 1. Editor proposes an update
    Editor->>OwnerPDS: getRecord(document) + getBlob(cid)
    Editor->>Editor: Decrypt, edit, re-encrypt
    Editor->>EditorPDS: uploadBlob(new ciphertext)
    Editor->>EditorPDS: createRecord(documentUpdate)

    EditorPDS->>Indexer: firehose event
    Indexer->>Indexer: validate editor role, index update

    Note over Owner,OwnerPDS: 2. Owner applies the update
    Owner->>Indexer: GET /api/workspace/updates
    Indexer-->>Owner: pending documentUpdate records
    Owner->>EditorPDS: getBlob(update cid)
    Owner->>OwnerPDS: uploadBlob + putRecord(document)

    Note over Editor,EditorPDS: 3. Cleanup
    Editor->>EditorPDS: deleteRecord(documentUpdate)
```

The owner's client is the only one that writes to the canonical document record. Editors propose changes; owners apply them. Last-write-wins by `createdAt` for conflict resolution.

## Flow: Leaving a workspace

```mermaid
sequenceDiagram
    participant Member
    participant MemberPDS as Member's PDS
    participant Indexer

    Member->>MemberPDS: createRecord(keyringUpdate, { keyring, actionType: "leave" })
    MemberPDS->>Indexer: firehose event
    Indexer->>Indexer: remove member from workspace index
    Note right of Indexer: Workspace disappears from<br/>member's sidebar
```

The member's wrapped key still exists on the keyring record — they *could* still decrypt. This is a visibility opt-out, not a key revocation. The owner can follow up with a proper removal (key rotation) if needed.

## Flow: Device-to-device identity pairing

```mermaid
sequenceDiagram
    participant DevB as Device B (new)
    participant PDS
    participant DevA as Device A (existing)

    Note over DevB,PDS: 1. New device creates pair request
    DevB->>DevB: Generate ephemeral hybrid keypair (X25519 + ML-KEM-768)
    DevB->>PDS: createRecord(pairRequest, { x25519EphemeralKey, mlKemEphemeralKey })
    DevB->>DevB: Persist both private halves to local Storage (32 + 2400 bytes, keyed by DID+rkey)
    DevB->>DevB: Display X25519 key fingerprint
    DevB->>DevB: Poll for pairResponse...

    Note over DevA,PDS: 2. Existing device approves
    DevA->>PDS: listRecords(pairRequest)
    PDS-->>DevA: Pending requests with fingerprints
    DevA->>DevA: User confirms matching fingerprint

    Note over DevA,PDS: 3. Existing device sends identity
    DevA->>DevA: Generate content key K
    DevA->>DevA: Serialize identity → JSON
    DevA->>DevA: Encrypt identity with K (AES-256-GCM)
    DevA->>DevA: Wrap K to ephemeral hybrid bundle (x25519-mlkem768-hkdf-a256kw-v2)
    DevA->>PDS: createRecord(pairResponse, { wrappedKey, ciphertext })

    Note over DevB,PDS: 4. New device receives identity
    DevB->>PDS: listRecords(pairResponse)
    PDS-->>DevB: Matching response
    DevB->>DevB: Load ephemeral private bundle from Storage
    DevB->>DevB: Unwrap K via hybrid decapsulation (X25519 + ML-KEM-768)
    DevB->>DevB: Decrypt identity JSON
    DevB->>PDS: getRecord(publicKey/self)
    DevB->>DevB: Verify public keys match published record
    DevB->>DevB: Save identity.json
    DevB->>DevB: Wipe pair state from Storage

    Note over DevB,PDS: 5. Cleanup
    DevB->>PDS: deleteRecord(pairRequest)
    DevB->>PDS: deleteRecord(pairResponse)
```

Both devices are authenticated to the same DID. The PDS is just a relay — the encryption is the access control. Ephemeral key fingerprints are displayed for visual SAS comparison.

For detailed sequence diagrams of every CLI operation, see [docs/flows/](../docs/flows/).
