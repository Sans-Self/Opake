# Directory Operations

## Create Directory

Creates a directory record and registers it as a child of the root.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake mkdir Photos

    CLI->>PDS: com.atproto.repo.getRecord (directory/self)
    alt Root exists
        PDS-->>CLI: root directory record
    else Root not found (404)
        CLI->>PDS: com.atproto.repo.putRecord (directory/self, name="/")
        PDS-->>CLI: { uri, cid }
    end

    CLI->>PDS: com.atproto.repo.createRecord (directory, name="Photos")
    PDS-->>CLI: { uri, cid }

    CLI->>PDS: com.atproto.repo.getRecord (root)
    CLI->>CLI: Append new directory URI to entries
    CLI->>PDS: com.atproto.repo.putRecord (root with updated entries)
    PDS-->>CLI: { uri, cid }

    CLI->>User: Photos → at://did/.../directory/<tid>
```

Directories are children-on-parent: the parent's `entries` array holds the AT-URIs of its children. The root directory is a singleton at rkey "self".

## Delete (Non-Recursive)

Deletes an empty directory. Refuses if the directory has entries.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake rm Photos

    Note over CLI: Fast path: try document resolution first
    CLI->>PDS: listRecords (document collection)
    PDS-->>CLI: no match → NotFound

    Note over CLI: Fall back to tree load (directories only)
    CLI->>PDS: listRecords (directory collection, paginated)
    PDS-->>CLI: all directories (includes root)

    CLI->>CLI: tree.resolve("Photos") → directory in memory
    CLI->>CLI: count_descendants → 0 docs, 0 dirs

    CLI->>User: delete Photos/? [y/N]
    User-->>CLI: y

    CLI->>PDS: com.atproto.repo.deleteRecord (directory)
    PDS-->>CLI: 200 OK

    CLI->>PDS: getRecord (root) → remove_entry → putRecord (root)
    PDS-->>CLI: 200 OK

    CLI->>User: deleted at://did/.../directory/<rkey>
```

## Delete (Recursive)

Deletes a directory and all its contents in post-order (children before parents).

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake rm -r Photos

    Note over CLI: Path contains no / → try document resolution
    CLI->>PDS: listRecords (document collection)
    PDS-->>CLI: no match → NotFound

    Note over CLI: Fall back to tree load (directories only)
    CLI->>PDS: listRecords (directory collection, paginated)
    PDS-->>CLI: all directories (includes root)

    CLI->>CLI: tree.resolve("Photos") → directory in memory
    CLI->>CLI: count_descendants → 2 docs, 1 subdir (from entry URIs)
    CLI->>User: delete Photos/? (2 documents, 1 subdirectories) [y/N]
    User-->>CLI: y

    CLI->>CLI: collect_descendants (post-order, from entry URIs)
    Note over CLI: Children deleted before parents<br/>No getRecord needed — URIs known from directory entries

    loop Each descendant (post-order)
        CLI->>PDS: com.atproto.repo.deleteRecord
        PDS-->>CLI: 200 OK
    end

    CLI->>PDS: com.atproto.repo.deleteRecord (Photos)
    PDS-->>CLI: 200 OK

    CLI->>PDS: getRecord (root) → remove_entry → putRecord (root)
    PDS-->>CLI: 200 OK

    CLI->>User: deleted at://...  (2 documents, 2 directories)
```

Post-order deletion means leaf documents are removed first, then empty subdirectories, then the target directory itself. The parent's entry list is only updated once (for the target directory — descendant directories are deleted wholesale without updating their parents' entries, since the parents are also being deleted).

## Path Resolution

The `DirectoryTree` resolves user-provided references to AT-URIs. Three input forms are supported:

| Input | Strategy | API cost |
|-------|----------|----------|
| `at://did/.../document/rkey` | Passthrough | 0 calls |
| `beach.jpg` (bare name) | `documents::resolve_uri` (fast path) | 1 paginated call |
| `Photos` (bare name, no document match) | Tree load + in-memory search | 1 paginated + 0 calls |
| `Photos/beach.jpg` | Tree load + walk + getRecord per doc child | 1 paginated + N getRecord |
| `Photos/Vacation/sunset.jpg` | Tree load + walk + getRecord per doc child | 1 paginated + N getRecord |

Bare names search only root's direct children (matching filesystem semantics). Use paths for nested items.

The tree is built from a single paginated `listRecords` call (directories only). Directory segments are walked in memory. When the final segment targets a document, each document child URI in the parent directory is fetched individually via `getRecord` to match by name. This avoids loading the entire document collection — only the children of the relevant directory are fetched.

For recursive deletion (`rm -r`), descendant counts and URIs are determined entirely from directory entry arrays — document names aren't needed, so no `getRecord` calls are made for counting or collecting.

Future optimization: a local URI → name cache (#155) would eliminate repeated `getRecord` calls for the same directory's children.
