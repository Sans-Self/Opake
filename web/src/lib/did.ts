// DID document resolution — JS fetch + WASM parsing.
//
// URL construction and document parsing are delegated to opake-core via WASM.
// This module handles the HTTP fetch (which WASM can't do).
//
// VITE_PLC_DIRECTORY_URL overrides the PLC directory base URL for testing
// (e.g. pointing at a fake-pds instance that serves DID documents).

import { getOpakeWorker } from "@/lib/worker";

const PLC_DIRECTORY_OVERRIDE = import.meta.env.VITE_PLC_DIRECTORY_URL as string | undefined;

/** Fetch and parse a DID document, returning the PDS URL. */
export async function pdsUrlFromDid(did: string): Promise<string> {
  const worker = getOpakeWorker();
  const url = PLC_DIRECTORY_OVERRIDE
    ? `${PLC_DIRECTORY_OVERRIDE}/${did}`
    : await worker.didDocumentUrl(did);

  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to fetch DID document for ${did}: HTTP ${response.status}`);
  }

  const docBytes = new Uint8Array(await response.arrayBuffer());
  return worker.pdsFromDidDocument(docBytes);
}

/** Fetch a DID document and extract the handle from `alsoKnownAs`. */
export async function handleFromDid(did: string): Promise<string | undefined> {
  const worker = getOpakeWorker();
  const url = PLC_DIRECTORY_OVERRIDE
    ? `${PLC_DIRECTORY_OVERRIDE}/${did}`
    : await worker.didDocumentUrl(did);

  const response = await fetch(url);
  if (!response.ok) return undefined;

  const docBytes = new Uint8Array(await response.arrayBuffer());
  return worker.handleFromDidDocument(docBytes);
}
