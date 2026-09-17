# definitions

## Purpose

The words this project uses, one requirement per term. The requirement
name is the term, the body its meaning, the scenarios show it in a
sentence, and a `- **Deprecated:**` line lists the words not to use for
it.

## Requirements

### Requirement: group key

The symmetric key that wraps a workspace's content keys for the current
rotation. Every member holds a wrap of it; rotation replaces it.

- **Deprecated:** workspace key, rotation key

#### Scenario: In a sentence

- **WHEN** a member is removed
- **THEN** the group key rotates

### Requirement: manager

The membership role that may add and remove members and author keyring
supersedes. The other roles are editor and viewer.

- **Deprecated:** admin

#### Scenario: In a sentence

- **WHEN** a manager removes a member
- **THEN** the keyring head changes
