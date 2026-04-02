// Base64, fingerprint, and AT-URI encoding utilities.

/** Standard base64 from bytes. */
export function uint8ArrayToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

/** Base64 to bytes — handles unpadded strings (PDS returns unpadded). */
export function base64ToUint8Array(b64: string): Uint8Array {
  // Pad to multiple of 4
  const padded = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/** First 8 bytes of a public key as a colon-separated hex fingerprint. */
export function formatFingerprint(pubkey: Uint8Array): string {
  return Array.from(pubkey.slice(0, 8))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join(":");
}

/** Extract the rkey from an AT-URI: `at://did/collection/rkey` → `rkey`. */
export function rkeyFromUri(atUri: string): string {
  const parts = atUri.split("/");
  const rkey = parts.at(-1);
  if (!rkey) throw new Error(`Invalid AT-URI: ${atUri}`);
  return rkey;
}
