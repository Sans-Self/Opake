// Device pairing — extracted from Opake class to keep the file manageable.
//
// All functions take a WASM context and return typed results.
// The Opake class delegates to these after token guard + context check.

import type { Identity } from "./storage";
import type { PendingPairRequest, PairResponseRecord, PairRequestResult } from "./types";
import { pairRequestResultSchema } from "./schemas";

type Ctx = import("../wasm/opake.js").OpakeContext;

function base64ToBytes(b64: string): Uint8Array {
  const padded = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

export function createPairRequest(ctx: Ctx): Promise<PairRequestResult> {
  return ctx.createPairRequest().then(pairRequestResultSchema.parse);
}

export function listPairRequests(ctx: Ctx): Promise<readonly PendingPairRequest[]> {
  return ctx.listPairRequests().then((entries: readonly {
    uri: string;
    value: { ephemeralKey: { $bytes: string }; createdAt: string };
  }[]) => entries.map((e) => ({
    uri: e.uri,
    ephemeralKey: base64ToBytes(e.value.ephemeralKey.$bytes),
    createdAt: e.value.createdAt,
  })));
}

export function listPairResponses(
  ctx: Ctx,
): Promise<readonly { uri: string; requestUri: string; value: PairResponseRecord }[]> {
  return ctx.listPairResponses().then((entries: readonly {
    uri: string;
    value: { request: string; [key: string]: unknown };
  }[]) => entries.map((e) => ({
    uri: e.uri,
    requestUri: e.value.request,
    value: e.value as PairResponseRecord,
  })));
}

export function approvePairRequest(ctx: Ctx, requestUri: string, ephemeralPublicKey: Uint8Array): Promise<void> {
  return ctx.approvePairRequest(requestUri, ephemeralPublicKey);
}

export function receivePairResponse(ctx: Ctx, response: PairResponseRecord, ephemeralPrivateKey: Uint8Array): Promise<Identity> {
  return ctx.receivePairResponse(response, ephemeralPrivateKey) as Promise<Identity>;
}

export function cleanupPairRecords(ctx: Ctx, requestRkey: string, responseRkey: string): Promise<void> {
  return ctx.cleanupPairRecords(requestRkey, responseRkey);
}

export function cleanupExpiredPairRequests(ctx: Ctx): Promise<number> {
  return ctx.cleanupExpiredPairRequests();
}
