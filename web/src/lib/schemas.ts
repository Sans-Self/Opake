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
  rootUri: z.string().nullable(),
  directories: z.record(z.string(), DirectorySnapshotEntrySchema),
});

// ---------------------------------------------------------------------------
// Storage types (IndexedDB boundary)
// ---------------------------------------------------------------------------

export const AccountEntrySchema = z.object({
  pdsUrl: z.string(),
  handle: z.string(),
});

export const ConfigSchema = z.object({
  defaultDid: z.string().nullable(),
  accounts: z.record(z.string(), AccountEntrySchema),
  cacheEnabled: z.boolean().optional(),
});

export const IdentitySchema = z.object({
  did: z.string(),
  public_key: z.string(),
  private_key: z.string(),
  signing_key: z.string().nullable(),
  verify_key: z.string().nullable(),
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
  privateKey: z.string(),
  publicJwk: DpopPublicJwkSchema,
});

export const LegacySessionSchema = z.object({
  type: z.literal("legacy"),
  did: z.string(),
  handle: z.string(),
  accessJwt: z.string(),
  refreshJwt: z.string(),
});

// Rust serializes as "oAuth" (camelCase rename_all), TS uses "oauth".
// Accept both and normalize to "oauth" to match the TS Session union.
export const OAuthSessionSchema = z.object({
  type: z.union([z.literal("oauth"), z.literal("oAuth")]).transform((): "oauth" => "oauth"),
  did: z.string(),
  handle: z.string(),
  accessToken: z.string(),
  refreshToken: z.string(),
  dpopKey: DpopKeyPairSchema,
  tokenEndpoint: z.string(),
  dpopNonce: z.string().nullable(),
  expiresAt: z.number().nullable(),
  clientId: z.string(),
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
    fetchedAt: z.number(),
  });

// ---------------------------------------------------------------------------
// Crypto types (WASM boundary)
// ---------------------------------------------------------------------------

export const PkceChallengeSchema = z.object({
  verifier: z.string(),
  challenge: z.string(),
});

export const EphemeralKeypairSchema = z.object({
  publicKey: Uint8ArraySchema,
  privateKey: Uint8ArraySchema,
});

// ---------------------------------------------------------------------------
// WASM PDS operation results (through flatten())
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

export const IncomingGrantSchema = z.object({
  uri: z.string(),
  ownerDid: z.string(),
  documentUri: z.string(),
  createdAt: z.string(),
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
