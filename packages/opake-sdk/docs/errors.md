# Error Handling

All SDK methods throw `OpakeError` on failure. Each error has a `.kind`
discriminant you can match on.

## OpakeError

```typescript
import { OpakeError } from "@opake/sdk";

try {
  await cabinet.download(documentUri);
} catch (e) {
  if (e instanceof OpakeError) {
    switch (e.kind) {
      case "NotFound":
        console.log("Document doesn't exist");
        break;
      case "Auth":
        console.log("Session expired — re-authenticate");
        break;
      case "Decryption":
        console.log("Can't decrypt — wrong key or corrupted data");
        break;
      default:
        console.error("Unexpected error:", e.kind, e.message);
    }
  }
}
```

## Error Kinds

| Kind | When | Recovery |
|------|------|----------|
| `NotFound` | Record or blob doesn't exist on the PDS | Check the URI, or the document was deleted |
| `Auth` | Session expired or token refresh failed | Re-authenticate via OAuth flow |
| `Encryption` | Encryption operation failed | Likely a key issue — check identity |
| `Decryption` | Decryption failed (wrong key, corrupted data) | Verify you have the correct content key |
| `KeyWrap` | Key wrapping/unwrapping failed | Recipient's public key may be wrong |
| `InvalidRecord` | PDS record doesn't match expected schema | Schema version mismatch or corrupted record |
| `Storage` | Storage backend failed (IndexedDB error, etc.) | Check storage implementation |
| `Xrpc` | PDS XRPC call failed | Check PDS connectivity, inspect `.message` for HTTP status |
| `Appview` | AppView API call failed | Check AppView connectivity |
| `AlreadyExists` | Tried to create something that already exists | Check before creating, or handle idempotently |
| `AmbiguousName` | Multiple documents match a name query | Use AT URIs instead of names |
| `Serialization` | JSON serialization/deserialization failed | Usually a bug — report it |
| `Mnemonic` | Invalid BIP-39 seed phrase | Check spelling, word count (24 words) |
| `Unknown` | Error didn't match a known kind | Inspect `.message` for details |

## Patterns

### Retry on Auth Errors

```typescript
async function withRetry<T>(fn: () => Promise<T>, reauthenticate: () => Promise<void>): Promise<T> {
  try {
    return await fn();
  } catch (e) {
    if (e instanceof OpakeError && e.kind === "Auth") {
      await reauthenticate();
      return fn();
    }
    throw e;
  }
}
```

### Distinguish User Errors from Bugs

```typescript
const USER_ERRORS: Set<string> = new Set(["NotFound", "Auth", "AlreadyExists", "Mnemonic"]);

function isUserError(e: unknown): boolean {
  return e instanceof OpakeError && USER_ERRORS.has(e.kind);
}
```
