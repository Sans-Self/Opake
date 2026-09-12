## 1. Freshness at write preparation

- [ ] 1.1 Refresh canonical workspace authority, rotation, and key before upload/edit preparation; verify a cached rotation-7 workspace uses observed rotation 8 or refuses without its key
- [ ] 1.2 Invalidate prepared encryption after an observed rotation change and recheck authority before publishing; verify controlled races never knowingly publish the discarded plan
- [ ] 1.3 Keep historical-only and unavailable-current-state failures explicit; verify no write path falls back to a historical group key

## 2. Fresh keys for changed plaintext

- [ ] 2.1 Audit workspace plaintext-changing entry points and use fresh content-key material; verify an old key remains unusable for changed content even when the old wrap was already swept forward
- [ ] 2.2 Rekey directory rename metadata and preserve lineage/AAD bindings; verify a removed member's old directory key cannot decrypt the new name
- [ ] 2.3 Rekey both blob and metadata for file edits under the shared-key format; verify round-trip decryption and no publication when a required blob is unavailable
- [ ] 2.4 Preserve unchanged-ciphertext cascade behavior without rotation-triggered blob work; verify ordinary removal touches no document blobs

## 3. Accepted race and documentation

- [ ] 3.1 Add a controlled old-key-upload/removal race through production APIs; verify the already-encrypted in-flight ciphertext remains decryptable to the removed member and is not reported as protected by later retry
- [ ] 3.2 Add the paired fresh-key case; verify a remaining informed writer's new ciphertext is not decryptable with removed-member key material
- [ ] 3.3 Update security and edit-cost documentation; verify no wall-clock cutoff, indexer-rejection-as-confidentiality, or whole-workspace re-encryption claim remains, and run OpenSpec validation plus spec-lint
