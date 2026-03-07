// IndexedDB-backed Storage implementation using Dexie.js.
// Mirrors: crates/opake-cli/src/config.rs — FileStorage (but for the browser)

import Dexie, { type EntityTable } from "dexie"
import type { Config, Identity, Session } from "./storage-types"
import { type Storage, StorageError, sanitizeDid } from "./storage"

const CONFIG_KEY = "global"

interface ConfigRow {
  key: string
  value: Config
}

interface IdentityRow {
  did: string
  value: Identity
}

interface SessionRow {
  did: string
  value: Session
}

class OpakeDatabase extends Dexie {
  readonly configs!: Readonly<EntityTable<ConfigRow, "key">>
  readonly identities!: Readonly<EntityTable<IdentityRow, "did">>
  readonly sessions!: Readonly<EntityTable<SessionRow, "did">>

  constructor(name = "opake") {
    super(name)
    this.version(1).stores({
      configs: "key",
      identities: "did",
      sessions: "did",
    })
  }
}

export class IndexedDbStorage implements Storage {
  private readonly db: Readonly<OpakeDatabase>

  constructor(dbName = "opake") {
    this.db = new OpakeDatabase(dbName)
  }

  async loadConfig(): Promise<Config> {
    const row = await this.db.configs.get(CONFIG_KEY)
    if (!row) {
      throw new StorageError("no config found — log in first")
    }
    return row.value
  }

  async saveConfig(config: Config): Promise<void> {
    await this.db.configs.put({ key: CONFIG_KEY, value: config })
  }

  async loadIdentity(did: string): Promise<Identity> {
    const key = sanitizeDid(did)
    const row = await this.db.identities.get(key)
    if (!row) {
      throw new StorageError(`no identity for ${did} — log in first`)
    }
    return row.value
  }

  async saveIdentity(did: string, identity: Identity): Promise<void> {
    const key = sanitizeDid(did)
    await this.db.identities.put({ did: key, value: identity })
  }

  async loadSession(did: string): Promise<Session> {
    const key = sanitizeDid(did)
    const row = await this.db.sessions.get(key)
    if (!row) {
      throw new StorageError(`no session for ${did} — log in first`)
    }
    return row.value
  }

  async saveSession(did: string, session: Session): Promise<void> {
    const key = sanitizeDid(did)
    await this.db.sessions.put({ did: key, value: session })
  }

  async removeAccount(did: string): Promise<void> {
    const config = await this.loadConfig()
    const remainingAccounts = Object.fromEntries(
      Object.entries(config.accounts).filter(([key]) => key !== did),
    )
    const remaining = Object.keys(remainingAccounts)
    const updatedConfig: Config = {
      ...config,
      accounts: remainingAccounts,
      defaultDid:
        config.defaultDid === did
          ? remaining.length > 0
            ? remaining[0]
            : null
          : config.defaultDid,
    }
    const key = sanitizeDid(did)
    await this.db.transaction(
      "rw",
      [this.db.configs, this.db.identities, this.db.sessions],
      async () => {
        await this.db.configs.put({ key: CONFIG_KEY, value: updatedConfig })
        await this.db.identities.delete(key)
        await this.db.sessions.delete(key)
      },
    )
  }

  /** Close the database connection. Useful for test cleanup. */
  close(): void {
    this.db.close()
  }

  /** Delete the entire database. Useful for test cleanup. */
  async destroy(): Promise<void> {
    this.db.close()
    await this.db.delete()
  }
}
