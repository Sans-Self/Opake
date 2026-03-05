# Opake Black-Box Test Plan

Manual end-to-end testing against live PDS instances. Covers every CLI command.

Do not read the code in this library before executing this test. Ask the user for input instead.

## Requirements

- **Accounts needed:** 2 minimum, 3 recommended
  - Account A: the primary user (uploads, creates keyrings, shares files)
  - Account B: a recipient (downloads shared files, joins keyrings)
  - Account C (optional): a third user to test multi-member keyrings and non-member rejection
- Accounts can be on the same PDS or different PDSes. Cross-PDS is the harder path and should be tested if possible.
- `cargo build` must succeed before starting.
- Each test section is independent — run them in order for a clean state, or cherry-pick.

## Notation

```
A$  = run as account A (opake --as <A-handle> ...)
B$  = run as account B (opake --as <B-handle> ...)
C$  = run as account C (opake --as <C-handle> ...)
```

Substitute real handles/DIDs for `<A-handle>`, `<B-handle>`, etc.

---

## 1. Login and Account Management

### 1.1 Login

```bash
opake login <A-handle>
# resolves PDS, authenticates via OAuth, prints "Logged in as <handle>"
# also publishes encryption public key (putRecord)

opake login <B-handle>
```

**Verify:**
- Login succeeds for both accounts
- `opake accounts` shows both
- One is marked as default

### 1.2 Accounts and switching

```bash
opake accounts
# lists all logged-in accounts with DID, handle, PDS URL, and default marker

opake set-default <B-handle>
opake accounts
# B is now the default

opake set-default <A-handle>
```

### 1.3 Logout

```bash
# don't actually log out yet — we need the accounts for later tests
# but verify the flag exists:
opake logout --help
```

### 1.4 Resolve

```bash
A$ opake resolve <B-handle>
# prints DID, PDS URL, public key, algorithm

A$ opake resolve <A-handle>
# resolving yourself should also work
```

**Verify:**
- Output includes DID, PDS URL, base64 public key, and `x25519` algorithm
- Resolving a nonexistent handle errors cleanly

---

## 2. Upload and Download (Direct Encryption)

### 2.1 Upload a file

```bash
echo "hello opake" > /tmp/test-direct.txt

A$ opake upload /tmp/test-direct.txt
# prints: test-direct.txt → at://did:plc:A/app.opake.cloud.document/<rkey>
```

Save the output AT-URI as `$DOC_URI`.

### 2.2 List documents

```bash
A$ opake ls
# shows test-direct.txt

A$ opake ls -l
# long format: size, mime type, tags, URI
```

**Verify:**
- File appears in the list
- Long format shows `text/plain`, size, and the AT-URI

### 2.3 Download own file

```bash
A$ opake download test-direct.txt -o /tmp/test-direct-download.txt
# prints: test-direct.txt → /tmp/test-direct-download.txt (12 bytes)

diff /tmp/test-direct.txt /tmp/test-direct-download.txt
# no output = identical
```

### 2.4 Download by AT-URI

```bash
A$ opake download $DOC_URI -o /tmp/test-direct-uri.txt
diff /tmp/test-direct.txt /tmp/test-direct-uri.txt
```

### 2.5 Download refuses to overwrite

```bash
A$ opake download test-direct.txt -o /tmp/test-direct-download.txt
# should error: "output file already exists"
```

### 2.6 Upload with tags

```bash
A$ opake upload /tmp/test-direct.txt --tags tax,2026
# new document with tags

A$ opake ls --tag tax
# only tagged document appears
```

### 2.7 Upload empty file

```bash
touch /tmp/empty.bin
A$ opake upload /tmp/empty.bin
A$ opake download empty.bin -o /tmp/empty-download.bin
# 0 bytes, no error
```

---

## 3. Directories

### 3.1 Create directories

```bash
A$ opake mkdir Photos
# prints: Photos → at://did:plc:A/app.opake.cloud.directory/<rkey>

A$ opake mkdir Archive
```

### 3.2 Upload into a directory

```bash
echo "beach photo" > /tmp/beach.jpg
A$ opake upload /tmp/beach.jpg --dir Photos
# prints: beach.jpg → at://... (in Photos)
```

### 3.3 View the tree

```bash
A$ opake tree
# /
# ├── Archive/
# ├── Photos/
# │   └── beach.jpg
# └── test-direct.txt    (if still present from section 2)
```

**Verify:**
- Directories appear with trailing `/`
- Documents appear as leaves
- Indentation and box-drawing characters are correct

### 3.4 Cat a file (decrypt to stdout)

```bash
A$ opake cat beach.jpg
# prints "beach photo" to stdout (no file created)

# path-based cat:
A$ opake cat Photos/beach.jpg
# same output
```

**Verify:**
- Output is the decrypted plaintext
- No prompt, no "saved to" message — just raw content

### 3.5 Move a file into a directory

```bash
echo "meeting notes" > /tmp/notes.txt
A$ opake upload /tmp/notes.txt
A$ opake mv notes.txt Archive/
# prints: moved "notes.txt" → Archive/

A$ opake tree
# Archive/ now contains notes.txt
```

### 3.6 Rename a file

```bash
A$ opake mv notes.txt meeting-notes.txt
# prints: renamed "notes.txt" → "meeting-notes.txt"
```

### 3.7 Move via path

```bash
A$ opake mv Archive/meeting-notes.txt Photos/
# prints: moved "meeting-notes.txt" → Photos/
```

### 3.8 Rename a directory

```bash
A$ opake mv Archive Old
# prints: renamed "Archive" → "Old"
```

### 3.9 Move into self is rejected

```bash
A$ opake mv Photos Photos/
# should error: "cannot move a directory into itself"
```

---

## 4. Delete

```bash
A$ opake rm beach.jpg
# prompts "delete beach.jpg? [y/N]"
# type y

A$ opake ls
# file is gone
```

### 4.1 Delete with --yes

```bash
A$ opake upload /tmp/test-direct.txt
A$ opake rm test-direct.txt -y
# no prompt, immediate delete
```

### 4.2 Delete by path

```bash
echo "delete me" > /tmp/deleteme.txt
A$ opake upload /tmp/deleteme.txt --dir Photos
A$ opake rm Photos/deleteme.txt -y
# deletes the document and removes it from Photos' entry list
```

### 4.3 Delete an empty directory

```bash
A$ opake rm Old -y
# deletes the directory (must be empty)
```

### 4.4 Delete non-empty directory without -r fails

```bash
A$ opake rm Photos
# should error: "directory is not empty (N documents, M subdirectories) — use -r to delete recursively"
```

### 4.5 Recursive delete

```bash
echo "sunset" > /tmp/sunset.jpg
A$ opake upload /tmp/sunset.jpg --dir Photos
A$ opake rm -r Photos
# prompts: "delete Photos/? (1 documents, 0 subdirectories) [y/N]"
# type y
# prints: deleted at://... (1 documents, 1 directories)

A$ opake tree
# Photos is gone
```

---

## 5. Sharing (Grants)

### 5.1 Upload a file to share

```bash
echo "shared secret" > /tmp/shared-file.txt
A$ opake upload /tmp/shared-file.txt
```

Save URI as `$SHARED_URI`.

### 5.2 Share with B

```bash
A$ opake share shared-file.txt <B-handle> --note "for your eyes only"
# prints: shared with <B-handle> → at://did:plc:A/app.opake.cloud.grant/<grant-rkey>
```

Save grant URI as `$GRANT_URI`.

### 5.3 List outgoing grants

```bash
A$ opake shared
# shows the grant: recipient, permissions, URI

A$ opake shared -l
# long format: document URI, grant URI, note
```

**Verify:**
- Grant appears with B's DID as recipient
- Note shows up in long format

### 5.4 Download via grant (cross-PDS)

```bash
B$ opake download --grant $GRANT_URI -o /tmp/shared-download.txt
# prints: shared-file.txt → /tmp/shared-download.txt (14 bytes)

diff /tmp/shared-file.txt /tmp/shared-download.txt
# identical
```

**Verify:**
- Works even though the file lives on A's PDS
- B never needs to be a "member" of anything

### 5.5 Revoke

```bash
A$ opake revoke $GRANT_URI
# prompts, type y
# prints: revoked at://...

A$ opake shared
# grant is gone
```

### 5.6 Download after revoke fails

```bash
B$ opake download --grant $GRANT_URI -o /tmp/should-fail.txt
# should error (404 or similar — grant record is deleted)
```

---

## 6. Keyrings

### 6.1 Create a keyring

```bash
A$ opake keyring create family-photos
# prints: family-photos → at://did:plc:A/app.opake.cloud.keyring/<kr-rkey>
```

### 6.2 List keyrings

```bash
A$ opake keyring ls
# family-photos  1 member(s)

A$ opake keyring ls -l
# includes URI and rotation count (0)
```

### 6.3 Upload under keyring

```bash
echo "family photo metadata" > /tmp/photo.txt
A$ opake upload /tmp/photo.txt --keyring family-photos
```

Save URI as `$KR_DOC_URI`.

### 6.4 Download own keyring-encrypted file

```bash
A$ opake download photo.txt -o /tmp/photo-download.txt
diff /tmp/photo.txt /tmp/photo-download.txt
# identical
```

### 6.5 Add member

```bash
A$ opake keyring add-member family-photos <B-handle>
# prints: added <B-handle> to family-photos

A$ opake keyring ls
# family-photos  2 member(s)
```

### 6.6 Member download (cross-PDS)

```bash
B$ opake download --keyring-member $KR_DOC_URI -o /tmp/kr-member-download.txt
# prints: photo.txt → /tmp/kr-member-download.txt (22 bytes)

diff /tmp/photo.txt /tmp/kr-member-download.txt
# identical
```

**Verify:**
- B fetched from A's PDS (unauthenticated)
- Group key is now cached locally for B

### 6.7 Subsequent downloads use cached key

Upload a second file under the same keyring as A:

```bash
echo "second family photo" > /tmp/photo2.txt
A$ opake upload /tmp/photo2.txt --keyring family-photos
```

Save URI as `$KR_DOC2_URI`.

B downloads without `--keyring-member` (uses cached group key):

```bash
B$ opake download --keyring-member $KR_DOC2_URI -o /tmp/photo2-download.txt
diff /tmp/photo2.txt /tmp/photo2-download.txt
```

### 6.8 Non-member is rejected

```bash
C$ opake download --keyring-member $KR_DOC_URI -o /tmp/should-fail.txt
# should error: "not a member of keyring"
```

If C is not available, skip this test.

### 6.9 Remove member

```bash
A$ opake keyring remove-member family-photos <B-handle>
# prompts about key rotation, type y
# prints: removed <B-handle> from family-photos (key rotated)

A$ opake keyring ls
# family-photos  1 member(s)

A$ opake keyring ls -l
# rotation count is now 1
```

### 6.10 Removed member cannot download new uploads

After removal, upload a new file under the rotated keyring:

```bash
echo "new secret after rotation" > /tmp/photo3.txt
A$ opake upload /tmp/photo3.txt --keyring family-photos
```

B's cached group key is from rotation 0; the document is wrapped under rotation 1:

```bash
B$ opake download --keyring-member <photo3-uri> -o /tmp/should-fail.txt
# should error (key unwrap failure — B's cached group key is stale,
# and B is no longer in the keyring members list)
```

---

## 7. Error Cases

### 7.1 Download nonexistent file

```bash
A$ opake download nonexistent-file.txt
# should error: not found
```

### 7.2 Invalid AT-URI

```bash
A$ opake download "not-a-uri"
# should error: AT-URI parse failure
```

### 7.3 Wrong account downloads direct file

```bash
B$ opake download $DOC_URI -o /tmp/wrong-account.txt
# should error: no wrapped key for DID
```

(Only works if A still has a direct-encrypted file uploaded. Re-upload one if needed.)

### 7.4 --grant and --keyring-member conflict

```bash
B$ opake download --grant at://x --keyring-member at://y
# should error: clap conflict (cannot use both flags)
```

### 7.5 --keyring-member on a direct-encrypted document

```bash
B$ opake download --keyring-member $SHARED_URI -o /tmp/should-fail.txt
# should error: "document uses direct encryption, not keyring"
```

(Use a URI for a direct-encrypted file on A's PDS.)

---

## 8. AppView

The AppView is a separate binary (`opake-appview`) that indexes grants and keyrings from the AT Protocol firehose. These tests require a running Jetstream instance or network access to the public Jetstream relays.

### 8.1 Configuration

Create a minimal config:

```bash
cat > /tmp/opake-appview-test/appview.toml <<EOF
jetstream_url = "wss://jetstream2.us-east.bsky.network/subscribe"
listen = "127.0.0.1:6100"
db_path = "/tmp/opake-appview-test/appview.db"
EOF
```

### 8.2 Status (cold start)

```bash
opake-appview --config-dir /tmp/opake-appview-test status
# Cursor:   (none — indexer has not run)
# Grants:   0
# Keyrings: 0
```

### 8.3 Start indexer + API

```bash
opake-appview --config-dir /tmp/opake-appview-test run -v &
APPVIEW_PID=$!
sleep 3
```

**Verify:**
- Logs show "opake-appview listening on 127.0.0.1:6100"
- Logs show Jetstream connection established

### 8.4 Health endpoint

```bash
curl -s http://127.0.0.1:6100/api/health | jq .
# {
#   "indexerConnected": true,
#   "cursorTime": "2026-...",
#   "cursorAgeSecs": <small number>
# }
```

**Verify:**
- `indexerConnected` is `true`
- `cursorAgeSecs` is small (< 60)

### 8.5 Inbox and keyrings require auth

```bash
curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:6100/api/inbox?did=did:plc:test
# 401

curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:6100/api/keyrings?did=did:plc:test
# 401
```

### 8.6 Share triggers indexing

With the AppView still running, create a share grant using the CLI (from section 4.2):

```bash
A$ opake share shared-file.txt <B-handle>
```

Wait a few seconds for the firehose to deliver the event.

```bash
opake-appview --config-dir /tmp/opake-appview-test status
# Grants: should be ≥ 1
```

### 8.7 Status (after indexing)

```bash
opake-appview --config-dir /tmp/opake-appview-test status
# Cursor:   2026-03-02T...
# Lag:      <small number>s
# Grants:   <non-zero>
# Keyrings: <number>
```

### 8.8 Config dir matches CLI

Both binaries should resolve the same config directory:

```bash
# Both use OPAKE_DATA_DIR
OPAKE_DATA_DIR=/tmp/opake-appview-test opake-appview status
# should find /tmp/opake-appview-test/appview.toml

# --config-dir flag works the same way
opake-appview --config-dir /tmp/opake-appview-test status
```

### 8.9 Cleanup

```bash
kill $APPVIEW_PID 2>/dev/null
rm -rf /tmp/opake-appview-test
```

---

## 9. Cleanup

```bash
# remove test files (some may already be deleted from section 4)
A$ opake rm photo.txt -y 2>/dev/null
A$ opake rm photo2.txt -y 2>/dev/null
A$ opake rm empty.bin -y 2>/dev/null
# etc.

rm /tmp/test-direct*.txt /tmp/shared-*.txt /tmp/photo*.txt /tmp/empty* /tmp/kr-* /tmp/should-fail.txt /tmp/beach.jpg /tmp/notes.txt /tmp/sunset.jpg /tmp/deleteme.txt 2>/dev/null

# optionally logout test accounts
opake logout <B-handle>
opake logout <C-handle>
```
