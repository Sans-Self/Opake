export type VerificationCallback = { code: string; state: string; issuer: string };

export function verificationChannelName(channel: string): string {
  return `opake-verification:${channel}`;
}

/** Accept only a complete callback. A partial redirect must not reach a live
 * operation, which keeps issuer/state binding inside the WASM/core driver. */
export function verificationCallbackFromSearch(search: string): VerificationCallback | null {
  const params = new URLSearchParams(search);
  const code = params.get("code");
  const state = params.get("state");
  const issuer = params.get("iss");
  return code && state && issuer ? { code, state, issuer } : null;
}
