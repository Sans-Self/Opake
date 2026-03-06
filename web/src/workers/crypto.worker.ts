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
  generateDpopKeyPair as wasmGenerateDpopKeyPair,
  createDpopProof as wasmCreateDpopProof,
  generatePkce as wasmGeneratePkce,
  generateIdentity as wasmGenerateIdentity,
  generateEphemeralKeypair as wasmGenerateEphemeralKeypair,
} from "@/wasm/opake-wasm/opake";
import type { EncryptedPayload, WrappedKey, DpopKeyPair, PkceChallenge, EphemeralKeypair } from "@/lib/crypto-types";
import type { Identity } from "@/lib/storage-types";

await init();

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

  decryptBlob(
    key: Uint8Array,
    ciphertext: Uint8Array,
    nonce: Uint8Array,
  ): Uint8Array {
    return decryptBlob(key, ciphertext, nonce);
  },

  wrapKey(
    contentKey: Uint8Array,
    recipientPubKey: Uint8Array,
    recipientDid: string,
  ): WrappedKey {
    return wrapKey(contentKey, recipientPubKey, recipientDid) as WrappedKey;
  },

  unwrapKey(wrappedKey: WrappedKey, privateKey: Uint8Array): Uint8Array {
    return unwrapKey(wrappedKey, privateKey);
  },

  wrapContentKeyForKeyring(
    contentKey: Uint8Array,
    groupKey: Uint8Array,
  ): Uint8Array {
    return wrapContentKeyForKeyring(contentKey, groupKey);
  },

  unwrapContentKeyFromKeyring(
    wrapped: Uint8Array,
    groupKey: Uint8Array,
  ): Uint8Array {
    return unwrapContentKeyFromKeyring(wrapped, groupKey);
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
};

export type CryptoApi = typeof cryptoApi;

Comlink.expose(cryptoApi);
