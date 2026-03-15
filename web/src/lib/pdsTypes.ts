// TypeScript equivalents of PDS record types returned by listRecords / getRecord.
// Mirrors: crates/opake-core/src/records/

import type { AtBytes, WrappedKey } from "./cryptoTypes";

// ---------------------------------------------------------------------------
// Generic listRecords response
// ---------------------------------------------------------------------------

export interface PdsRecord<T> {
  readonly uri: string;
  readonly cid: string;
  readonly value: T;
}

export interface ListRecordsResponse<T> {
  readonly records: readonly PdsRecord<T>[];
  readonly cursor?: string;
}

// ---------------------------------------------------------------------------
// app.opake.accountConfig
// ---------------------------------------------------------------------------

export interface AccountConfigRecord {
  readonly opakeVersion: number;
  readonly telemetryEnabled: boolean;
  readonly appviewUrl?: string;
  readonly modifiedAt: string;
}

// ---------------------------------------------------------------------------
// Encryption envelope (shared by documents, directories, keyrings, grants)
// ---------------------------------------------------------------------------

export interface EncryptedMetadataEnvelope {
  readonly ciphertext: AtBytes;
  readonly nonce: AtBytes;
}

export interface EncryptionEnvelope {
  readonly algo: string;
  readonly nonce: AtBytes;
  readonly keys: readonly WrappedKey[];
}

interface DirectEncryption {
  readonly $type: "app.opake.document#directEncryption";
  readonly envelope: EncryptionEnvelope;
}

interface KeyringEncryption {
  readonly $type: "app.opake.document#keyringEncryption";
  readonly keyringRef: {
    readonly keyring: string;
    readonly wrappedContentKey: AtBytes;
    readonly rotation: number;
  };
  readonly algo: string;
  readonly nonce: AtBytes;
}

export type Encryption = DirectEncryption | KeyringEncryption;

// ---------------------------------------------------------------------------
// app.opake.document
// ---------------------------------------------------------------------------

export interface BlobRef {
  readonly $type: "blob";
  readonly ref: { readonly $link: string };
  readonly mimeType: string;
  readonly size: number;
}

export interface DocumentRecord {
  readonly opakeVersion: number;
  readonly blob: BlobRef;
  readonly encryption: Encryption;
  readonly encryptedMetadata: EncryptedMetadataEnvelope;
  readonly visibility: string | null;
  readonly createdAt: string;
  readonly modifiedAt: string | null;
}

// ---------------------------------------------------------------------------
// app.opake.directory
// ---------------------------------------------------------------------------

export interface DirectoryRecord {
  readonly opakeVersion: number;
  readonly encryption: Encryption;
  readonly encryptedMetadata: EncryptedMetadataEnvelope;
  readonly entries: string[];
  readonly createdAt: string;
  readonly modifiedAt: string | null;
}

// ---------------------------------------------------------------------------
// Decrypted metadata (result of worker decryption)
// ---------------------------------------------------------------------------

export interface DocumentMetadata {
  readonly name: string;
  readonly mimeType?: string;
  readonly size?: number;
  readonly tags?: string[];
  readonly description?: string;
}

export interface DirectoryMetadata {
  readonly name: string;
  readonly description?: string;
}

// ---------------------------------------------------------------------------
// DirectoryTree snapshot (returned by WASM DirectoryTreeHandle.snapshot())
// ---------------------------------------------------------------------------

export interface DirectorySnapshotEntry {
  readonly name: string;
  readonly entries: readonly string[];
}

export interface DirectoryTreeSnapshot {
  readonly rootUri: string | null;
  readonly directories: Readonly<Record<string, DirectorySnapshotEntry>>;
}
