/// SQL statements to initialize the database schema.
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS cursor (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    time_us INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS grants (
    uri TEXT PRIMARY KEY,
    owner_did TEXT NOT NULL,
    recipient_did TEXT NOT NULL,
    document_uri TEXT NOT NULL,
    created_at TEXT NOT NULL,
    indexed_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_grants_recipient ON grants (recipient_did);
CREATE INDEX IF NOT EXISTS idx_grants_owner ON grants (owner_did);

CREATE TABLE IF NOT EXISTS keyring_members (
    keyring_uri TEXT NOT NULL,
    member_did TEXT NOT NULL,
    owner_did TEXT NOT NULL,
    indexed_at TEXT NOT NULL,
    PRIMARY KEY (keyring_uri, member_did)
);
CREATE INDEX IF NOT EXISTS idx_keyring_members_did ON keyring_members (member_did);
";
