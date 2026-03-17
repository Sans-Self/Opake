// Identity, auth, and DID operations — keypair generation, seed phrases,
// DPoP proofs, PKCE, appview signing, DID document resolution.

import {
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
} from "@/wasm/opake-wasm/opake";
import type { DpopKeyPair, PkceChallenge, EphemeralKeypair } from "@/lib/cryptoTypes";
import type { AccountConfigRecord } from "@/lib/pdsTypes";
import type { Identity } from "@/lib/storageTypes";

export const identityApi = {
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

  signAppviewRequest(
    method: string,
    path: string,
    did: string,
    signingKey: Uint8Array,
    timestamp: number,
  ): string {
    return wasmSignAppviewRequest(method, path, did, signingKey, timestamp);
  },

  didDocumentUrl(did: string): string {
    return wasmDidDocumentUrl(did);
  },

  handleFromDidDocument(docJson: Uint8Array): string | undefined {
    return wasmHandleFromDidDocument(docJson);
  },

  pdsFromDidDocument(docJson: Uint8Array): string {
    return wasmPdsFromDidDocument(docJson);
  },

  accountConfigCollection(): string {
    return wasmAccountConfigCollection();
  },

  accountConfigRkey(): string {
    return wasmAccountConfigRkey();
  },

  newAccountConfig(modifiedAt: string): AccountConfigRecord {
    return wasmNewAccountConfig(modifiedAt) as AccountConfigRecord;
  },
};
