/** Extract the authority DID from an AT Protocol URI (`at://did:plc:xxx/...`). */
export function didFromUri(uri: string): string {
  return uri.split("/")[2];
}

/** Extract the rkey (last path segment) from an AT Protocol URI. */
export function rkeyFromUri(uri: string): string {
  const segments = uri.split("/");
  return segments[segments.length - 1];
}

/** Build a full AT URI for a directory record. */
export function directoryUri(did: string, rkey: string): string {
  return `at://${did}/app.opake.directory/${rkey}`;
}

/** Build a full AT URI for a document record. */
export function documentUri(did: string, rkey: string): string {
  return `at://${did}/app.opake.document/${rkey}`;
}
