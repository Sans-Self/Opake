// Shared shape for decrypted preview payloads. The caller passes a
// decrypt thunk to <FilePreview /> — the mechanics of where the
// FileManager comes from belong with the caller, not this module.

export interface DecryptedBlob {
  readonly plaintext: Uint8Array;
  readonly metadata: { readonly name: string; readonly mimeType?: string };
}
