// Platform-agnostic storage contract.
// Mirrors: crates/opake-core/src/storage.rs — Storage trait

import type { Config, Identity, Session } from "./storage-types"

export interface Storage {
  loadConfig(): Promise<Config>
  saveConfig(config: Config): Promise<void>
  loadIdentity(did: string): Promise<Identity>
  saveIdentity(did: string, identity: Identity): Promise<void>
  loadSession(did: string): Promise<Session>
  saveSession(did: string, session: Session): Promise<void>
  removeAccount(did: string): Promise<void>
}

export class StorageError extends Error {
  constructor(message: string) {
    super(message)
    this.name = "StorageError"
  }
}

/** `did:plc:abc` → `did_plc_abc` — mirrors `sanitize_did` in opake-core. */
export function sanitizeDid(did: string): string {
  return did.replaceAll(":", "_")
}
