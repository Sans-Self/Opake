// OAuth scope registry — canonical list of collections and scope construction.
//
// Adding a new at.opake.* collection? Add it to OPAKE_COLLECTIONS below.
// The OAuth scope string and the permission set lexicon are both derived
// from this list.

/// All `at.opake.*` collections that Opake needs repo access to.
///
/// This is the single source of truth for the OAuth scope string.
/// The compile-time test at the bottom of this file ensures every
/// `*_COLLECTION` const in the crate appears here.
pub const OPAKE_COLLECTIONS: &[&str] = &[
    crate::records::ACCOUNT_CONFIG_COLLECTION,
    crate::directories::DIRECTORY_COLLECTION,
    crate::documents::DOCUMENT_COLLECTION,
    crate::sharing::GRANT_COLLECTION,
    crate::keyrings::KEYRING_COLLECTION,
    crate::records::PAIR_REQUEST_COLLECTION,
    crate::records::PAIR_RESPONSE_COLLECTION,
    crate::records::PENDING_SHARE_COLLECTION,
    crate::records::PUBLIC_KEY_COLLECTION,
];

/// Build the OAuth scope string for Opake.
///
/// Requests granular per-collection repo access for all `at.opake.*` record
/// types, blob upload/download access, and the base `atproto` scope.
pub fn oauth_scope() -> String {
    let mut parts: Vec<String> = Vec::with_capacity(OPAKE_COLLECTIONS.len() + 2);
    parts.push("atproto".into());
    for collection in OPAKE_COLLECTIONS {
        parts.push(format!("repo:{collection}"));
    }
    parts.push("blob:*/*".into());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensures every *_COLLECTION const in the crate is listed in
    /// OPAKE_COLLECTIONS. If this fails, you added a new collection
    /// constant but forgot to register it for OAuth scopes.
    // spec:auth-session § The OAuth scope derives from one collection registry
    #[test]
    fn all_collection_constants_are_registered() {
        let all_known: &[&str] = &[
            crate::records::ACCOUNT_CONFIG_COLLECTION,
            crate::directories::DIRECTORY_COLLECTION,
            crate::documents::DOCUMENT_COLLECTION,
            crate::sharing::GRANT_COLLECTION,
            crate::keyrings::KEYRING_COLLECTION,
            crate::records::PAIR_REQUEST_COLLECTION,
            crate::records::PAIR_RESPONSE_COLLECTION,
            crate::records::PENDING_SHARE_COLLECTION,
            crate::records::PUBLIC_KEY_COLLECTION,
        ];

        for collection in all_known {
            assert!(
                OPAKE_COLLECTIONS.contains(collection),
                "collection {collection} is not registered in OPAKE_COLLECTIONS — \
                 add it to crate::scope::OPAKE_COLLECTIONS so it's included in the OAuth scope",
            );
        }
    }

    #[test]
    fn oauth_scope_includes_base_and_blob() {
        let scope = oauth_scope();
        assert!(scope.starts_with("atproto "));
        assert!(scope.ends_with(" blob:*/*"));
    }

    // spec:auth-session § The OAuth scope derives from one collection registry
    #[test]
    fn oauth_scope_includes_all_collections() {
        let scope = oauth_scope();
        for collection in OPAKE_COLLECTIONS {
            assert!(
                scope.contains(&format!("repo:{collection}")),
                "scope missing repo:{collection}",
            );
        }
    }
}
