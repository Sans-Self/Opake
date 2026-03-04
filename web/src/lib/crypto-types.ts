export interface AtBytes {
  $bytes: string;
}

export interface WrappedKey {
  did: string;
  ciphertext: AtBytes;
  algo: string;
}

export interface EncryptedPayload {
  ciphertext: Uint8Array;
  nonce: Uint8Array;
}
