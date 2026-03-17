// Pure crypto operations — encrypt, decrypt, wrap, unwrap.

import {
  generateContentKey,
  encryptBlob,
  decryptBlob,
  wrapKey,
  unwrapKey,
  wrapContentKeyForKeyring,
  unwrapContentKeyFromKeyring,
  encryptMetadata as wasmEncryptMetadata,
  decryptMetadata as wasmDecryptMetadata,
  encryptDirectoryMetadata as wasmEncryptDirectoryMetadata,
  decryptDirectoryMetadata as wasmDecryptDirectoryMetadata,
  schemaVersion as wasmSchemaVersion,
} from "@/wasm/opake-wasm/opake";
import type { EncryptedPayload, WrappedKey } from "@/lib/cryptoTypes";
import type { DocumentMetadata, DirectoryMetadata } from "@/lib/pdsTypes";

export const cryptoApi = {
  schemaVersion(): number {
    return wasmSchemaVersion();
  },

  generateContentKey(): Uint8Array {
    return generateContentKey();
  },

  encryptBlob(key: Uint8Array, plaintext: Uint8Array): EncryptedPayload {
    return encryptBlob(key, plaintext) as EncryptedPayload;
  },

  decryptBlob(key: Uint8Array, ciphertext: Uint8Array, nonce: Uint8Array): Uint8Array {
    return decryptBlob(key, ciphertext, nonce);
  },

  wrapKey(contentKey: Uint8Array, recipientPubKey: Uint8Array, recipientDid: string): WrappedKey {
    return wrapKey(contentKey, recipientPubKey, recipientDid) as WrappedKey;
  },

  unwrapKey(wrappedKey: WrappedKey, privateKey: Uint8Array): Uint8Array {
    return unwrapKey(wrappedKey, privateKey);
  },

  wrapContentKeyForKeyring(contentKey: Uint8Array, groupKey: Uint8Array): Uint8Array {
    return wrapContentKeyForKeyring(contentKey, groupKey);
  },

  unwrapContentKeyFromKeyring(wrapped: Uint8Array, groupKey: Uint8Array): Uint8Array {
    return unwrapContentKeyFromKeyring(wrapped, groupKey);
  },

  encryptMetadata(key: Uint8Array, metadata: DocumentMetadata): EncryptedPayload {
    return wasmEncryptMetadata(key, metadata) as EncryptedPayload;
  },

  decryptMetadata(key: Uint8Array, ciphertext: Uint8Array, nonce: Uint8Array): DocumentMetadata {
    return wasmDecryptMetadata(key, ciphertext, nonce) as DocumentMetadata;
  },

  encryptDirectoryMetadata(key: Uint8Array, metadata: DirectoryMetadata): EncryptedPayload {
    return wasmEncryptDirectoryMetadata(key, metadata) as EncryptedPayload;
  },

  decryptDirectoryMetadata(
    key: Uint8Array,
    ciphertext: Uint8Array,
    nonce: Uint8Array,
  ): DirectoryMetadata {
    return wasmDecryptDirectoryMetadata(key, ciphertext, nonce) as DirectoryMetadata;
  },
};
