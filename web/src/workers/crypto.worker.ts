import * as Comlink from "comlink";
import init, {
  bindingCheck,
  generateContentKey,
  encryptBlob,
  decryptBlob,
  wrapKey,
  unwrapKey,
  wrapContentKeyForKeyring,
  unwrapContentKeyFromKeyring,
  decryptMetadata as wasmDecryptMetadata,
  decryptDirectoryMetadata as wasmDecryptDirectoryMetadata,
  generateDpopKeyPair as wasmGenerateDpopKeyPair,
  createDpopProof as wasmCreateDpopProof,
  generatePkce as wasmGeneratePkce,
  generateIdentity as wasmGenerateIdentity,
  generateEphemeralKeypair as wasmGenerateEphemeralKeypair,
  DirectoryTreeHandle,
} from "@/wasm/opake-wasm/opake";
import type {
  EncryptedPayload,
  WrappedKey,
  DpopKeyPair,
  PkceChallenge,
  EphemeralKeypair,
} from "@/lib/cryptoTypes";
import type {
  DocumentMetadata,
  DirectoryMetadata,
  DirectoryTreeSnapshot,
  PdsRecord,
  DirectoryRecord,
} from "@/lib/pdsTypes";
import type { Identity } from "@/lib/storageTypes";

console.debug("[worker] initializing WASM");
await init();
console.debug("[worker] ready, binding check:", bindingCheck());

// ---------------------------------------------------------------------------
// Stateful directory tree held across calls
// ---------------------------------------------------------------------------

// eslint-disable-next-line functional/no-let -- stateful WASM handle held across calls
let directoryTree: DirectoryTreeHandle | null = null;

const cryptoApi = {
  ping(): string {
    return "pong";
  },

  bindingCheck(): string {
    return bindingCheck();
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

  // Metadata decryption

  decryptMetadata(key: Uint8Array, ciphertext: Uint8Array, nonce: Uint8Array): DocumentMetadata {
    return wasmDecryptMetadata(key, ciphertext, nonce) as DocumentMetadata;
  },

  decryptDirectoryMetadata(
    key: Uint8Array,
    ciphertext: Uint8Array,
    nonce: Uint8Array,
  ): DirectoryMetadata {
    return wasmDecryptDirectoryMetadata(key, ciphertext, nonce) as DirectoryMetadata;
  },

  // OAuth / DPoP

  generateDpopKeyPair(): DpopKeyPair {
    return wasmGenerateDpopKeyPair() as DpopKeyPair;
  },

  createDpopProof(
    keypair: DpopKeyPair,
    method: string,
    url: string,
    timestamp: number,
    nonce: string | null,
    accessToken: string | null,
  ): string {
    return wasmCreateDpopProof(
      keypair,
      method,
      url,
      timestamp,
      nonce ?? undefined,
      accessToken ?? undefined,
    );
  },

  generatePkce(): PkceChallenge {
    return wasmGeneratePkce() as PkceChallenge;
  },

  generateIdentity(did: string): Identity {
    return wasmGenerateIdentity(did) as Identity;
  },

  generateEphemeralKeypair(): EphemeralKeypair {
    return wasmGenerateEphemeralKeypair() as EphemeralKeypair;
  },

  // ---------------------------------------------------------------------------
  // Directory tree (stateful — single instance held in worker)
  // ---------------------------------------------------------------------------

  buildDirectoryTree(
    records: readonly PdsRecord<DirectoryRecord>[],
    did: string,
    privateKey: Uint8Array,
  ): DirectoryTreeSnapshot {
    if (directoryTree) {
      directoryTree.free();
      directoryTree = null;
    }

    const input = records.map((r) => ({ uri: r.uri, value: r.value }));
    directoryTree = new DirectoryTreeHandle(input, did, privateKey);
    return directoryTree.snapshot() as DirectoryTreeSnapshot;
  },

  treeRootUri(): string | undefined {
    return directoryTree?.rootUri();
  },

  treeEntriesFor(uri: string): readonly string[] | null {
    return (directoryTree?.entriesFor(uri) as string[] | null) ?? null;
  },

  treeDirectoryName(uri: string): string | undefined {
    return directoryTree?.directoryName(uri);
  },

  treeIsDirectory(uri: string): boolean {
    return directoryTree?.isDirectory(uri) ?? false;
  },

  treeFindParent(uri: string): string | undefined {
    return directoryTree?.findParent(uri);
  },

  destroyDirectoryTree(): void {
    if (directoryTree) {
      directoryTree.free();
      directoryTree = null;
    }
  },
};

export type CryptoApi = typeof cryptoApi;

Comlink.expose(cryptoApi);
