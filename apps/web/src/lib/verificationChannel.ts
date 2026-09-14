export type VerificationCallback = { code: string; state: string; issuer: string };

export function verificationChannelName(channel: string): string {
  return `opake-verification:${channel}`;
}

/** A callback channel is a fresh UUID minted by the settings page. */
export function isVerificationChannelId(value: string | null): value is string {
  return value !== null && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value);
}

/** OAuth may leave the app for HTTPS PDSes, or an explicit local dev PDS. */
export function authorizationDestination(value: string): string | null {
  try {
    const url = new URL(value);
    const local = url.hostname === "localhost" || url.hostname === "127.0.0.1" || url.hostname.endsWith(".test");
    if (url.username || url.password || (url.protocol !== "https:" && !(url.protocol === "http:" && local))) return null;
    return url.toString();
  } catch { return null; }
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
