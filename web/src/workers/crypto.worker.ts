import * as Comlink from "comlink";
import init, {
  bindingCheck,
  schemaVersion as wasmSchemaVersion,
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
  generateDpopKeyPair as wasmGenerateDpopKeyPair,
  createDpopProof as wasmCreateDpopProof,
  generatePkce as wasmGeneratePkce,
  generateIdentity as wasmGenerateIdentity,
  generateMnemonic as wasmGenerateMnemonic,
  validateMnemonic as wasmValidateMnemonic,
  deriveIdentityFromMnemonic as wasmDeriveIdentityFromMnemonic,
  generateEphemeralKeypair as wasmGenerateEphemeralKeypair,
  signAppviewRequest as wasmSignAppviewRequest,
  didDocumentUrl as wasmDidDocumentUrl,
  handleFromDidDocument as wasmHandleFromDidDocument,
  pdsFromDidDocument as wasmPdsFromDidDocument,
  accountConfigCollection as wasmAccountConfigCollection,
  accountConfigRkey as wasmAccountConfigRkey,
  newAccountConfig as wasmNewAccountConfig,
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
  AccountConfigRecord,
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

  // Metadata encryption / decryption

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

  generateMnemonic(): string {
    return wasmGenerateMnemonic();
  },

  validateMnemonic(phrase: string): boolean {
    return wasmValidateMnemonic(phrase);
  },

  deriveIdentityFromMnemonic(phrase: string, did: string): Identity {
    return wasmDeriveIdentityFromMnemonic(phrase, did) as Identity;
  },

  generateEphemeralKeypair(): EphemeralKeypair {
    return wasmGenerateEphemeralKeypair() as EphemeralKeypair;
  },

  // ---------------------------------------------------------------------------
  // Account config
  // ---------------------------------------------------------------------------

  accountConfigCollection(): string {
    return wasmAccountConfigCollection();
  },

  accountConfigRkey(): string {
    return wasmAccountConfigRkey();
  },

  newAccountConfig(modifiedAt: string): AccountConfigRecord {
    return wasmNewAccountConfig(modifiedAt) as AccountConfigRecord;
  },

  // ---------------------------------------------------------------------------
  // AppView auth signing
  // ---------------------------------------------------------------------------

  signAppviewRequest(
    method: string,
    path: string,
    did: string,
    signingKey: Uint8Array,
    timestamp: number,
  ): string {
    return wasmSignAppviewRequest(method, path, did, signingKey, timestamp);
  },

  // ---------------------------------------------------------------------------
  // DID document utilities
  // ---------------------------------------------------------------------------

  didDocumentUrl(did: string): string {
    return wasmDidDocumentUrl(did);
  },

  handleFromDidDocument(docJson: Uint8Array): string | undefined {
    return wasmHandleFromDidDocument(docJson);
  },

  pdsFromDidDocument(docJson: Uint8Array): string {
    return wasmPdsFromDidDocument(docJson);
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

  treeCountDescendants(uri: string): { documents: number; directories: number } {
    if (!directoryTree) return { documents: 0, directories: 0 };
    return directoryTree.countDescendants(uri) as { documents: number; directories: number };
  },

  treeCollectDescendants(uri: string): readonly { uri: string; kind: string }[] {
    if (!directoryTree) return [];
    return directoryTree.collectDescendants(uri) as { uri: string; kind: string }[];
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
