# workspace-sequencing Specification

## Purpose

Give a workspace an auditable order and a freshness beacon without hiring a trusted sequencer. The substrate offers no trusted ordering: firehose cursors are per-connection and unsigned, a repo's `rev` is a self-attested clock that only orders one repo against itself, and verified timestamps are a discussion, not a shipped feature. But `did:plc` already proves the ecosystem accepts a centralized-but-auditable sequencer — it orders user-signed operations and publishes a self-certifying append-only log, and it is trusted only to maintain the accepted set, never to forge. That trust model is ours.

So we copy the blueprint, not the building: an append-only Merkle log of ingested record CIDs per workspace, published as signed tree heads with inclusion and consistency proofs — the certificate-transparency shape. It buys three things the trustless core cannot: an auditable order for tie-breaks, a freshness beacon that makes the fork-timing ceiling checkable, and omission upgraded from undetectable to visible. It sits on top of everything else and is never necessary for correctness. A self-hoster who runs no log loses those three and keeps a correct workspace.

## ADDED Requirements

### Requirement: The transparency log is an append-only Merkle log of ingested record CIDs

A sequencer MAY maintain, per workspace, an append-only Merkle log whose leaves are the CIDs of the workspace records it has ingested. The log SHALL be append-only: a published entry is never reordered or removed, and the log's structure SHALL support inclusion proofs (a leaf is in the tree under a given root) and consistency proofs (a newer root is an extension of an older one, not a rewrite).

The log records *what was ingested and in what order the sequencer saw it*. It asserts nothing about a record's validity; validity is decided independently by every client from the record's own signature and the authority rules.

#### Scenario: a leaf's inclusion is provable

- **WHEN** a client asks whether its record is in the log under the current root
- **THEN** the sequencer returns an inclusion proof the client verifies against the signed root, or the record is demonstrably absent

#### Scenario: a rewritten log fails its consistency proof

- **WHEN** a sequencer publishes a new root that drops or reorders a previously published leaf
- **THEN** any client holding an earlier signed root detects the break, because no valid consistency proof links the two roots

### Requirement: A signed tree head is the freshness beacon

The sequencer SHALL publish signed tree heads: a signature over `{workspace, Merkle root, sequence position}` that any client can verify. A fresh signed tree head is the freshness beacon — a checkable statement that, as of this head, the workspace's ingested frontier was at this root. This is what makes the fork-timing ceiling enforceable: without a beacon, "how far behind the head is this write's parent" is unanswerable, because time is trusted nowhere (`spec:workspace-membership § The fork-timing ceiling is measured against the frontier`).

A tree head establishes a frontier position, never a wall-clock time. Freshness remains a liveness property (`spec:workspace § Stated limitations no construction removes`): the beacon bounds staleness, it does not prove currency.

#### Scenario: the beacon bounds a cold joiner's staleness

- **GIVEN** a joiner holding a signed tree head conveyed with their invite
- **WHEN** a hostile host serves the joiner a state whose frontier is behind that head
- **THEN** the joiner refuses it, because the served frontier does not reach the beacon's root

#### Scenario: the beacon does not assert a clock

- **WHEN** a client reads a signed tree head
- **THEN** it derives only a frontier position from it, and makes no wall-clock claim about when that position was reached

### Requirement: The sequencer's tree-head key is roster-attested

A signed tree head is verified against a role-scoped Ed25519 key for the log-keeper — the same signature primitive as member records (`spec:record-signatures § Every workspace record carries an author signature`), so no second algorithm enters the system. That key SHALL be carried in the workspace roster, attested when a sequencer is designated exactly as a member's key is attested at add time (`spec:workspace-membership § The roster carries each member's signing key`); a cold joiner MAY receive it in the signed tree head conveyed with their invite, before they hold the roster. It is a distinct role key, never a human member's identity key, so replacing the log-keeper never touches anyone's identity.

The key is available to every party that syncs the roster and mandatory for none: only a client using the freshness beacon, or a witness cosigning a head, consults it, and a workspace running no log carries no such key (`§ The log is never necessary for truth`). Witnesses cosign with their own already-attested member keys, so cosigning introduces no further keys. The log-keeper's *private* key lives only with whoever runs the log; where the indexer runs the log it holds that role key alone, never a member's identity key.

#### Scenario: a beacon verifies against the roster-attested key

- **WHEN** a client checks a signed tree head
- **THEN** it verifies the signature against the log-keeper's role-scoped Ed25519 key carried in the roster — or the key conveyed with its invite before the roster is held — and never against an external document

#### Scenario: a workspace with no sequencer carries no sequencer key

- **WHEN** a workspace runs no transparency log
- **THEN** its roster carries no log-keeper key, and no correctness path consults one

### Requirement: The log is never necessary for truth

No correctness property of a workspace SHALL depend on a sequencer or its log. The layering is: the trustless compare-and-swap rule at the bottom, human endorsement above it, the auditable log on top when present. A client with no log SHALL fall back to the structural defence — a member holding a live frontier already refuses a fork rooted behind it — losing the cold-joiner protection and the tightened fork ceiling, never correctness.

The sequencer occupies the semi-trusted tier (`spec:workspace § The trust surface is four-tiered, and time is trusted nowhere`). Every assertion it makes is recomputable by clients from the records themselves; nothing it says is accepted on trust.

#### Scenario: a workspace with no sequencer is fully correct

- **WHEN** a workspace runs with no transparency log at all
- **THEN** membership, authority, and read access all function, and only the freshness beacon and auditable tie-break order are unavailable

#### Scenario: a client recomputes rather than trusts

- **WHEN** the sequencer reports an order or an inclusion result
- **THEN** the client verifies the proof and applies the deterministic rules itself, rather than accepting the sequencer's answer as authoritative

### Requirement: The log cannot make an invalid record valid

Ingestion into the log SHALL NOT confer validity. A record that fails its author signature, its authority check, or its vocabulary check is invalid whether or not it appears in the log, and a client SHALL reject it on those grounds regardless of its inclusion proof. The log's only power over truth is omission — it can decline to include a record — and omission is made detectable by consistency proofs, never curable by inclusion.

#### Scenario: an included forgery is still rejected

- **WHEN** the log contains a leaf for a record whose author signature does not verify
- **THEN** every client rejects the record, and its presence in the log changes nothing

### Requirement: Witness cosigning detects sequencer equivocation

The sequencer MAY be hardened against equivocation — showing one tree head to one member and a divergent head to another — by witness cosigning: members' own daemons countersign the heads they observe (the C2SP `tlog-witness` shape). Two cosigned heads that are inconsistent SHALL be treated as proof the sequencer equivocated, on the same footing as an author's double-supersede (`spec:record-signatures § Equivocation is self-incriminating`). Cosigning *detects* divergence; it does not defeat every equivocation. A sequencer that shows every witness the *same* stale head produces no divergence to catch, so uniform staleness evades cosigning and is bounded only by beacon freshness (`§ A signed tree head is the freshness beacon`), not by this mechanism. Witness cosigning is optional hardening; its absence weakens the beacon's trustworthiness for the memoryless, it does not break correctness.

#### Scenario: divergent cosigned heads convict the sequencer

- **GIVEN** two signed tree heads for the same workspace and sequence position, each cosigned by a distinct honest witness, that fail a consistency proof against each other
- **WHEN** any party holds both
- **THEN** it can prove the sequencer equivocated, using only the two cosigned heads

#### Scenario: uniform staleness evades cosigning

- **GIVEN** a sequencer that serves every witness the same stale-but-internally-consistent tree head
- **WHEN** the witnesses cosign the heads they observe
- **THEN** the cosignatures agree and detect no equivocation, because there is no divergence — the staleness is bounded by beacon freshness, not caught by cosigning

