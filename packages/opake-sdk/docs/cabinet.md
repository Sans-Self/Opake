# Cabinet Operations

Your cabinet is your personal encrypted file space. Files are encrypted to
your own X25519 public key — only you can decrypt them.

## File Operations

### Upload

```typescript
const cabinet = opake.cabinet();

const data = new Uint8Array(await file.arrayBuffer());
const result = await cabinet.upload(data, "report.pdf", "application/pdf", "Q3 financials");

console.log(result.uri);      // at://did:plc:.../app.opake.document/...
console.log(result.proposed);  // false (cabinet ops are always direct)
```

The `description` parameter is optional — it's stored in the encrypted metadata
envelope alongside the filename.

### Download

```typescript
const { filename, data } = await cabinet.download(documentUri);

// Create a browser download
const blob = new Blob([data]);
const url = URL.createObjectURL(blob);
```

### Delete

```typescript
await cabinet.delete(documentUri, parentDirectoryUri);
```

Pass the parent directory URI to clean up the directory entry. If omitted,
the document record is deleted but the directory still references it (stale
entry, cleaned up on next tree load).

### Update Content

Replace a document's encrypted blob without changing its URI or metadata:

```typescript
await cabinet.updateContent(documentUri, newData);
```

### Update Metadata

Change filename, description, or tags without re-uploading the file:

```typescript
await cabinet.updateMetadata(documentUri, {
  filename: "renamed-report.pdf",
  tags: ["finance", "q3"],
  description: "Updated Q3 report",
});
```

All fields are optional — only provided fields are changed.

## Directories

Directories are organizational containers with encrypted names. They don't
contain encryption keys — the document's own content key handles that.

### Create

```typescript
const result = await cabinet.createDirectory("photos");
console.log(result.uri);  // directory AT URI

// Nested directory
await cabinet.createDirectory("vacation", parentDirectoryUri);
```

### Ensure Root

The root directory is created automatically on first use, but you can
ensure it exists explicitly:

```typescript
const rootUri = await cabinet.ensureRoot();
```

### Load Tree

```typescript
const tree = await cabinet.loadTree();

// tree.rootUri — URI of the root directory (null if none)
// tree.directories — map of URI → { name, entries }

if (tree.rootUri) {
  const root = tree.directories[tree.rootUri];
  for (const entryUri of root.entries) {
    const subdir = tree.directories[entryUri];
    if (subdir) {
      console.log("Directory:", subdir.name);
    } else {
      console.log("Document:", entryUri);
    }
  }
}
```

`loadTree()` is read-only — it loads from the local cache and syncs deltas
from the AppView, but does not apply proposals or write to the PDS.

For the full sync cycle (apply proposals, resolve metadata), use
`syncAndLoadTree()`:

```typescript
const { snapshot, metadata } = await cabinet.syncAndLoadTree("*");

// metadata is a map of document URI → { name, mimeType, size, tags, description }
for (const [uri, meta] of Object.entries(metadata)) {
  console.log(meta.name, meta.mimeType, meta.size);
}
```

### Rename

```typescript
await cabinet.renameDirectory(directoryUri, "new-name");
```

### Move

Move a document or directory between parents:

```typescript
await cabinet.move(entryUri, sourceDirUri, targetDirUri);
```

### Delete Recursively

```typescript
const { documentsDeleted, directoriesDeleted } = await cabinet.deleteRecursive(directoryUri);
```

## Sharing

Share a document with another user by creating a grant — the document's
content key is wrapped to the recipient's X25519 public key.

### Create a Share

```typescript
// First, resolve the recipient's identity
const recipient = await opake.resolveIdentity("alice.bsky.social");

// Create the grant
await cabinet.share(
  documentUri,
  recipient.did,
  recipient.publicKey,
  "read",
  "Here's that report you asked for",  // optional note
);
```

### Revoke a Share

```typescript
await cabinet.revokeShare(grantUri);
```

Revoking deletes the grant record. The recipient can no longer decrypt
new copies, but if they downloaded the file before revocation, they still
have the plaintext. This is the same model as git-crypt — true revocation
requires re-encrypting the blob with a new content key.
