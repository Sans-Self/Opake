// Real-time event streaming from the Opake appview via Server-Sent Events.
//
// Opens an SSE connection authenticated via a short-lived token (obtained
// from the appview's Ed25519-signed token endpoint). Auto-reconnects with
// exponential backoff; fires `onReconnect` so the consumer can full-sync
// to cover the gap.

import { z } from "zod";

// ---------------------------------------------------------------------------
// SSE event schemas (validate appview payloads at the boundary)
// ---------------------------------------------------------------------------

export const sseDirectorySchema = z.object({
  directory_uri: z.string(),
  owner_did: z.string(),
  entries: z.array(z.unknown()).default([]),
  encrypted_metadata: z.unknown().nullish(),
  key_wrapping: z.unknown().nullish(),
  keyring_uri: z.string().nullish(),
  deleted_at: z.string().nullish(),
  indexed_at: z.string().nullish(),
});

export const sseDocumentSchema = z.object({
  document_uri: z.string(),
  owner_did: z.string(),
  encrypted_metadata: z.unknown().nullish(),
  encryption: z.unknown().nullish(),
  blob_ref: z.unknown().nullish(),
  keyring_uri: z.string().nullish(),
  rotation: z.number().nullish(),
  deleted_at: z.string().nullish(),
  indexed_at: z.string().nullish(),
});

export const sseKeyringSchema = z.object({
  uri: z.string(),
  owner_did: z.string(),
  rotation: z.number().nullish(),
  member_entries: z.array(z.unknown()).default([]),
  encrypted_metadata: z.unknown().nullish(),
  created_at: z.string().nullish(),
  indexed_at: z.string().nullish(),
});

export const sseGrantSchema = z.object({
  uri: z.string(),
  owner_did: z.string(),
  recipient_did: z.string().nullish(),
  document_uri: z.string(),
  created_at: z.string().nullish(),
});

export const sseDeleteSchema = z.object({
  uri: z.string().optional(),
  directory_uri: z.string().optional(),
  document_uri: z.string().optional(),
});

// Proposal schemas — workspace change proposals pending owner approval.
// All proposals carry the same identity fields (uri + author + keyring);
// individual schemas extend the base with their type-specific payload.

const sseProposalBaseSchema = z.object({
  uri: z.string(),
  author_did: z.string(),
  keyring_uri: z.string().nullish(),
});

export const sseDirectoryUpdateSchema = sseProposalBaseSchema.extend({
  action_type: z.string(),
  directory_uri: z.string().nullish(),
  entry_uri: z.string().nullish(),
  encrypted_metadata: z.unknown().nullish(),
  source_directory_uri: z.string().nullish(),
  target_directory_uri: z.string().nullish(),
  parent_directory_uri: z.string().nullish(),
});

export const sseKeyringUpdateSchema = sseProposalBaseSchema.extend({
  action_type: z.string(),
  member_did: z.string().nullish(),
  member_public_key: z.string().nullish(),
  role: z.string().nullish(),
  encrypted_metadata: z.unknown().nullish(),
});

export const sseDocumentUpdateSchema = sseProposalBaseSchema.extend({
  document_uri: z.string(),
  supersedes_uri: z.string().nullish(),
});

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type SSEDirectory = z.output<typeof sseDirectorySchema>;
export type SSEDocument = z.output<typeof sseDocumentSchema>;
export type SSEKeyring = z.output<typeof sseKeyringSchema>;
export type SSEGrant = z.output<typeof sseGrantSchema>;
export type SSEDelete = z.output<typeof sseDeleteSchema>;
export type SSEDirectoryUpdate = z.output<typeof sseDirectoryUpdateSchema>;
export type SSEKeyringUpdate = z.output<typeof sseKeyringUpdateSchema>;
export type SSEDocumentUpdate = z.output<typeof sseDocumentUpdateSchema>;

/** Handlers for SSE events. All optional — subscribe to what you need. */
export interface EventStreamHandlers {
  readonly onDirectoryUpsert?: (data: SSEDirectory) => void;
  readonly onDirectoryDelete?: (data: SSEDelete) => void;
  readonly onDocumentUpsert?: (data: SSEDocument) => void;
  readonly onDocumentDelete?: (data: SSEDelete) => void;
  readonly onKeyringUpsert?: (data: SSEKeyring) => void;
  readonly onKeyringDelete?: (data: SSEDelete) => void;
  readonly onGrantUpsert?: (data: SSEGrant) => void;
  readonly onGrantDelete?: (data: SSEDelete) => void;
  readonly onDirectoryUpdateUpsert?: (data: SSEDirectoryUpdate) => void;
  readonly onDirectoryUpdateDelete?: (data: SSEDelete) => void;
  readonly onKeyringUpdateUpsert?: (data: SSEKeyringUpdate) => void;
  readonly onKeyringUpdateDelete?: (data: SSEDelete) => void;
  readonly onDocumentUpdateUpsert?: (data: SSEDocumentUpdate) => void;
  readonly onDocumentUpdateDelete?: (data: SSEDelete) => void;
  /** Fired on reconnect — consumer should perform a full sync to cover the gap. */
  readonly onReconnect?: () => void;
  readonly onError?: (error: Error) => void;
  readonly onOpen?: () => void;
}

/** Configuration for an EventStream. */
export interface EventStreamOptions {
  /** Appview base URL (e.g., "https://appview.opake.app"). */
  readonly appviewUrl: string;
  /** Async function that returns a fresh single-use SSE token. Called on every connect/reconnect. */
  readonly getToken: () => Promise<string>;
  /** Event handlers. */
  readonly handlers: EventStreamHandlers;
  /** Max reconnect delay in ms (default: 30000). */
  readonly maxReconnectDelay?: number;
}

// ---------------------------------------------------------------------------
// EventStream
// ---------------------------------------------------------------------------

/**
 * Real-time event stream from the Opake appview.
 *
 * Connects via Server-Sent Events, authenticated with a short-lived token.
 * Auto-reconnects with exponential backoff on disconnect.
 *
 * @example
 * ```typescript
 * const stream = new EventStream({
 *   appviewUrl: "http://localhost:6100",
 *   getToken: () => opake.requestSseToken(),
 *   handlers: {
 *     onDirectoryUpsert: (dir) => console.log("directory changed", dir),
 *     onReconnect: () => store.fullSync(),
 *   },
 * });
 * await stream.connect();
 * // later:
 * stream.close();
 * ```
 */
export class EventStream {
  private eventSource: EventSource | null = null;
  private readonly appviewUrl: string;
  private readonly getToken: () => Promise<string>;
  private readonly handlers: EventStreamHandlers;
  private readonly maxReconnectDelay: number;
  private reconnectDelay = 1000;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;
  private wasConnected = false;

  constructor(options: EventStreamOptions) {
    this.appviewUrl = options.appviewUrl;
    this.getToken = options.getToken;
    this.handlers = options.handlers;
    this.maxReconnectDelay = options.maxReconnectDelay ?? 30_000;
  }

  /** Open the SSE connection. Obtains a fresh token first. */
  async connect(): Promise<void> {
    if (this.closed) return;

    try {
      const token = await this.getToken();
      if (this.closed) return; // re-check after async gap (StrictMode cleanup race)

      const url = `${this.appviewUrl}/api/events?token=${encodeURIComponent(token)}`;
      const es = new EventSource(url);
      this.eventSource = es;

      es.onopen = () => {
        this.reconnectDelay = 1000;
        this.wasConnected = true;
        this.handlers.onOpen?.();
      };

      es.onerror = () => {
        es.close();
        this.eventSource = null;
        this.scheduleReconnect();
      };

      // Register typed event listeners
      this.on(es, "directory:upsert", sseDirectorySchema, this.handlers.onDirectoryUpsert);
      this.on(es, "directory:delete", sseDeleteSchema, this.handlers.onDirectoryDelete);
      this.on(es, "document:upsert", sseDocumentSchema, this.handlers.onDocumentUpsert);
      this.on(es, "document:delete", sseDeleteSchema, this.handlers.onDocumentDelete);
      this.on(es, "keyring:upsert", sseKeyringSchema, this.handlers.onKeyringUpsert);
      this.on(es, "keyring:delete", sseDeleteSchema, this.handlers.onKeyringDelete);
      this.on(es, "grant:upsert", sseGrantSchema, this.handlers.onGrantUpsert);
      this.on(es, "grant:delete", sseDeleteSchema, this.handlers.onGrantDelete);
      // Proposal events (workspace change proposals pending owner approval)
      this.on(es, "directory_update:upsert", sseDirectoryUpdateSchema, this.handlers.onDirectoryUpdateUpsert);
      this.on(es, "directory_update:delete", sseDeleteSchema, this.handlers.onDirectoryUpdateDelete);
      this.on(es, "keyring_update:upsert", sseKeyringUpdateSchema, this.handlers.onKeyringUpdateUpsert);
      this.on(es, "keyring_update:delete", sseDeleteSchema, this.handlers.onKeyringUpdateDelete);
      this.on(es, "document_update:upsert", sseDocumentUpdateSchema, this.handlers.onDocumentUpdateUpsert);
      this.on(es, "document_update:delete", sseDeleteSchema, this.handlers.onDocumentUpdateDelete);
    } catch (e) {
      this.handlers.onError?.(e instanceof Error ? e : new Error(String(e)));
      this.scheduleReconnect();
    }
  }

  /** Close the connection and stop reconnecting. */
  close(): void {
    this.closed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.eventSource?.close();
    this.eventSource = null;
  }

  /** Whether the connection is currently open. */
  get connected(): boolean {
    return this.eventSource?.readyState === EventSource.OPEN;
  }

  // -- Internal --

  private on<T>(
    es: EventSource,
    eventType: string,
    schema: z.ZodType<T>,
    handler?: (data: T) => void,
  ): void {
    if (!handler) return;
    es.addEventListener(eventType, ((e: MessageEvent) => {
      try {
        const parsed = schema.parse(JSON.parse(e.data as string));
        handler(parsed);
      } catch (err) {
        this.handlers.onError?.(
          err instanceof Error ? err : new Error(`Failed to parse ${eventType} event`),
        );
      }
    }) as EventListener);
  }

  private scheduleReconnect(): void {
    if (this.closed) return;
    // Only fire onReconnect after a previously-successful connection drops —
    // not on initial connection failure where there's no gap to sync.
    if (this.wasConnected) this.handlers.onReconnect?.();
    // Jitter ±20% to prevent thundering herd on server restart
    const jitter = this.reconnectDelay * (0.8 + Math.random() * 0.4);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay);
      void this.connect();
    }, jitter);
  }
}
