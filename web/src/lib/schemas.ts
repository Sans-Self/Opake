// Zod schemas for runtime validation at trust boundaries (WASM, IndexedDB, API).
//
// These validate the same shapes as the interfaces in storageTypes.ts,
// pdsTypes.ts, and cryptoTypes.ts. They're used at boundaries where
// untyped data enters TypeScript — WASM returns, IndexedDB reads, HTTP
// responses — replacing `as T` casts with .parse() calls.

import { z } from "zod";

// Uint8ArraySchema fails across worker/main-thread boundaries
// because each realm has its own Uint8Array constructor. ArrayBuffer.isView
// works cross-realm.
const Uint8ArraySchema = z.custom<Uint8Array>(
  (val) => ArrayBuffer.isView(val) && val.constructor.name === "Uint8Array",
  "Expected Uint8Array",
);

// ---------------------------------------------------------------------------
// Primitives (AT Protocol building blocks)
// ---------------------------------------------------------------------------

export const AtBytesSchema = z.object({ $bytes: z.string() });

export const WrappedKeySchema = z.object({
  did: z.string(),
  ciphertext: AtBytesSchema,
  algo: z.string(),
});

export const BlobRefSchema = z.object({
  $type: z.literal("blob"),
  ref: z.object({ $link: z.string() }),
  mimeType: z.string(),
  size: z.number(),
});

export const EncryptedMetadataEnvelopeSchema = z.object({
  ciphertext: AtBytesSchema,
  nonce: AtBytesSchema,
});

// ---------------------------------------------------------------------------
// Encryption (discriminated union)
// ---------------------------------------------------------------------------

const EncryptionEnvelopeSchema = z.object({
  algo: z.string(),
  nonce: AtBytesSchema,
  keys: z.array(WrappedKeySchema),
});

const DirectEncryptionSchema = z.object({
  $type: z.literal("app.opake.document#directEncryption"),
  envelope: EncryptionEnvelopeSchema,
});

const KeyringEncryptionSchema = z.object({
  $type: z.literal("app.opake.document#keyringEncryption"),
  keyringRef: z.object({
    keyring: z.string(),
    wrappedContentKey: AtBytesSchema,
    rotation: z.number(),
  }),
  algo: z.string(),
  nonce: AtBytesSchema,
});

export const EncryptionSchema = z.discriminatedUnion("$type", [
  DirectEncryptionSchema,
  KeyringEncryptionSchema,
]);

// ---------------------------------------------------------------------------
// PDS record types
// ---------------------------------------------------------------------------

export const DocumentRecordSchema = z.object({
  opakeVersion: z.number(),
  blob: BlobRefSchema,
  encryption: EncryptionSchema,
  encryptedMetadata: EncryptedMetadataEnvelopeSchema,
  visibility: z.string().nullable(),
  createdAt: z.string(),
  modifiedAt: z.string().nullable(),
});

export const DirectoryRecordSchema = z.object({
  opakeVersion: z.number(),
  encryption: EncryptionSchema,
  encryptedMetadata: EncryptedMetadataEnvelopeSchema,
  entries: z.array(z.string()),
  createdAt: z.string(),
  modifiedAt: z.string().nullable(),
});

export const GrantRecordSchema = z.object({
  opakeVersion: z.number(),
  document: z.string(),
  recipient: z.string(),
  wrappedKey: WrappedKeySchema,
  encryptedMetadata: EncryptedMetadataEnvelopeSchema,
  expiresAt: z.string().optional(),
  createdAt: z.string(),
});

export const AccountConfigRecordSchema = z.object({
  opakeVersion: z.number(),
  telemetryEnabled: z.boolean(),
  appviewUrl: z.string().optional(),
  modifiedAt: z.string(),
});

// ---------------------------------------------------------------------------
// Generic PDS record wrapper
// ---------------------------------------------------------------------------

export const PdsRecordSchema = <T extends z.ZodType>(valueSchema: T) =>
  z.object({
    uri: z.string(),
    cid: z.string(),
    value: valueSchema,
  });

export const RecordRefSchema = z.object({
  uri: z.string(),
  cid: z.string(),
});

export const ListRecordsResponseSchema = <T extends z.ZodType>(valueSchema: T) =>
  z.object({
    records: z.array(PdsRecordSchema(valueSchema)),
    cursor: z.string().optional(),
  });

// ---------------------------------------------------------------------------
// Decrypted metadata
// ---------------------------------------------------------------------------

export const DocumentMetadataSchema = z.object({
  name: z.string(),
  mimeType: z.string().optional(),
  size: z.number().optional(),
  tags: z.array(z.string()).optional(),
  description: z.string().optional(),
});

export const DirectoryMetadataSchema = z.object({
  name: z.string(),
  description: z.string().optional(),
});

// ---------------------------------------------------------------------------
// Directory tree snapshot (from WASM DirectoryTreeHandle)
// ---------------------------------------------------------------------------

export const DirectorySnapshotEntrySchema = z.object({
  name: z.string(),
  entries: z.array(z.string()),
});

export const DirectoryTreeSnapshotSchema = z.object({
  root_uri: z.string().optional(),
  directories: z.record(z.string(), DirectorySnapshotEntrySchema),
});

// ---------------------------------------------------------------------------
// Storage types (IndexedDB boundary)
// ---------------------------------------------------------------------------

export const AccountEntrySchema = z.object({
  pds_url: z.string(),
  handle: z.string(),
});

export const ConfigSchema = z.object({
  default_did: z.string().optional(),
  accounts: z.record(z.string(), AccountEntrySchema),
  cache_enabled: z.boolean().optional(),
});

export const IdentitySchema = z.object({
  did: z.string(),
  public_key: z.string(),
  private_key: z.string(),
  signing_key: z.string().optional(),
  verify_key: z.string().optional(),
});

// ---------------------------------------------------------------------------
// Session (discriminated union)
// ---------------------------------------------------------------------------

export const DpopPublicJwkSchema = z.object({
  kty: z.string(),
  crv: z.string(),
  x: z.string(),
  y: z.string(),
});

export const DpopKeyPairSchema = z.object({
  private_key_b64: z.string(),
  public_jwk: DpopPublicJwkSchema,
});

export const LegacySessionSchema = z.object({
  type: z.literal("legacy"),
  did: z.string(),
  handle: z.string(),
  access_jwt: z.string(),
  refresh_jwt: z.string(),
});

export const OAuthSessionSchema = z.object({
  type: z.literal("oauth"),
  did: z.string(),
  handle: z.string(),
  access_token: z.string(),
  refresh_token: z.string(),
  dpop_key: DpopKeyPairSchema,
  token_endpoint: z.string(),
  dpop_nonce: z.string().optional(),
  expires_at: z.number().optional(),
  client_id: z.string(),
});

export const SessionSchema = z.union([LegacySessionSchema, OAuthSessionSchema]);

// ---------------------------------------------------------------------------
// Cache types
// ---------------------------------------------------------------------------

// CachedRecord and PdsRecord are structurally identical ({ uri, cid, value: T }).
export const CachedRecordSchema = PdsRecordSchema;

export const CachedCollectionSchema = <T extends z.ZodType>(valueSchema: T) =>
  z.object({
    records: z.array(CachedRecordSchema(valueSchema)),
    fetched_at: z.number(),
  });

// ---------------------------------------------------------------------------
// Crypto types (WASM boundary)
// ---------------------------------------------------------------------------

export const PkceChallengeSchema = z.object({
  verifier: z.string(),
  challenge: z.string(),
});

export const EphemeralKeypairSchema = z.object({
  public_key: Uint8ArraySchema,
  private_key: Uint8ArraySchema,
});

// ---------------------------------------------------------------------------
// Tree proposals (from AppView sync, member-verified)
// ---------------------------------------------------------------------------

export const TreeProposalSchema = z.object({
  uri: z.string(),
  author_did: z.string(),
  action_type: z.string(),
  directory_uri: z.string().optional(),
  entry_uri: z.string().optional(),
  encrypted_metadata: z.unknown().optional(),
  source_directory_uri: z.string().optional(),
  target_directory_uri: z.string().optional(),
  parent_directory_uri: z.string().optional(),
  indexed_at: z.string(),
});

// ---------------------------------------------------------------------------
// FileManager results (no session — signoff persists to IndexedDB)
// ---------------------------------------------------------------------------

/** Upload, delete, move, createDirectory → mutation outcome. */
export const MutationResultSchema = z.object({
  uri: z.string().optional(),
  proposed: z.boolean(),
});

/** Download → filename + decrypted plaintext. */
export const DownloadResultSchema2 = z.object({
  filename: z.string(),
  plaintext: z.custom<Uint8Array>(
    (val) => ArrayBuffer.isView(val) && val.constructor.name === "Uint8Array",
    "Expected Uint8Array",
  ),
});

/** Recursive delete → counts of deleted items. */
export const DeleteRecursiveResultSchema = z.object({
  documents_deleted: z.number(),
  directories_deleted: z.number(),
});

/** Grant metadata resolution (no blob download). */
export const GrantMetadataResultSchema = z.object({
  name: z.string(),
  metadata: DocumentMetadataSchema,
});

/** Pair request creation result. */
export const PairRequestResultSchema = z.object({
  uri: z.string(),
  rkey: z.string(),
  ephemeral_public_key: z.custom<Uint8Array>(
    (val) => ArrayBuffer.isView(val) && val.constructor.name === "Uint8Array",
    "Expected Uint8Array",
  ),
  ephemeral_private_key: z.custom<Uint8Array>(
    (val) => ArrayBuffer.isView(val) && val.constructor.name === "Uint8Array",
    "Expected Uint8Array",
  ),
});

/** Workspace create → keyring URI + group key. */
export const WorkspaceCreateResultSchema = z.object({
  keyring_uri: z.string(),
  key: z.custom<Uint8Array>(
    (val) => ArrayBuffer.isView(val) && val.constructor.name === "Uint8Array",
    "Expected Uint8Array",
  ),
});

/** Workspace list → keyrings array with decrypted metadata from WASM. */
export const WorkspaceListResultSchema = z.object({
  keyrings: z.array(
    z.object({
      uri: z.string(),
      ownerDid: z.string(),
      rotation: z.number(),
      memberCount: z.number(),
      createdAt: z.string().optional().nullable(),
      name: z.string().optional().nullable(),
      description: z.string().optional().nullable(),
      icon: z.string().optional().nullable(),
      members: z.array(z.unknown()),
    }),
  ),
});

/** Grant entry from listShares. */
export const GrantEntrySchema = z.object({
  uri: z.string(),
  document: z.string(),
  recipient: z.string(),
  encrypted_metadata: EncryptedMetadataEnvelopeSchema,
  expires_at: z.string().nullable().optional(),
  created_at: z.string(),
});

// ---------------------------------------------------------------------------
// Legacy WASM results (old flatten() pattern — pending store migration cleanup)
// ---------------------------------------------------------------------------

/** Base result shape: most WASM PDS operations return at least a session. */
const withSession = <T extends z.ZodRawShape>(shape: T) =>
  z.object({ ...shape, session: z.unknown() });

export const SessionResultSchema = withSession({});
export const UriResultSchema = withSession({ uri: z.string() });
export const DownloadResultSchema = withSession({
  filename: z.string(),
  plaintext: Uint8ArraySchema,
});
export const ContentKeyResultSchema = withSession({
  contentKey: Uint8ArraySchema,
});
export const MetadataUpdateResultSchema = withSession({
  metadata: z.unknown(),
});
export const UpdateContentResultSchema = withSession({
  modifiedAt: z.string(),
});

export const IncomingGrantSchema = z.object({
  uri: z.string(),
  owner_did: z.string(),
  document_uri: z.string(),
  created_at: z.string(),
});

export const IncomingGrantsResultSchema = withSession({
  grants: z.array(IncomingGrantSchema),
});

export const RawRecordEntrySchema = z.object({
  uri: z.string(),
  cid: z.string(),
  value: z.unknown(),
});

export const RawListResultSchema = withSession({
  records: z.array(RawRecordEntrySchema),
});

export const RawGetRecordResultSchema = withSession({
  record: RawRecordEntrySchema,
});

export const DownloadFromGrantResultSchema = z.object({
  filename: z.string(),
  plaintext: Uint8ArraySchema,
});

// ---------------------------------------------------------------------------
// Tree operation results
// ---------------------------------------------------------------------------

export const DescendantCountSchema = z.object({
  documents: z.number(),
  directories: z.number(),
});

export const DescendantEntrySchema = z.object({
  uri: z.string(),
  kind: z.string(),
});

// ---------------------------------------------------------------------------
// OAuth types (API boundary)
// ---------------------------------------------------------------------------

export const TokenResponseSchema = z.object({
  access_token: z.string(),
  token_type: z.string(),
  refresh_token: z.string().optional(),
  expires_in: z.number().optional(),
  scope: z.string().optional(),
  sub: z.string().optional(),
});

export const CachedProfileSchema = z.object({
  avatarUrl: z.string().nullable(),
  bannerUrl: z.string().nullable(),
  fetchedAt: z.number(),
});
