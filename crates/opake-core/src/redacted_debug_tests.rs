use crate::crypto::{encrypt_blob, generate_content_key, OsRng};

// -- Test structs --

#[derive(crate::RedactedDebug)]
struct NamedAllRedacted {
    #[redact]
    secret: String,
    #[redact]
    key: Vec<u8>,
}

#[derive(crate::RedactedDebug)]
struct NamedMixed {
    public: String,
    #[redact]
    secret: String,
    visible: u32,
}

#[derive(crate::RedactedDebug)]
struct NamedWithOption {
    #[redact]
    maybe_secret: Option<String>,
}

#[derive(crate::RedactedDebug)]
struct RedactedNewtype(#[redact] [u8; 16]);

#[derive(crate::RedactedDebug)]
struct TransparentNewtype(u32);

// -- Named struct tests --

#[test]
fn named_hides_all_redacted_fields() {
    let s = NamedAllRedacted {
        secret: "hunter2".into(),
        key: vec![0xAB; 64],
    };
    let out = format!("{s:?}");
    assert!(!out.contains("hunter2"), "secret leaked: {out}");
    assert!(out.contains("[7 bytes]"), "expected length: {out}");
    assert!(out.contains("[64 bytes]"), "expected length: {out}");
}

#[test]
fn named_shows_public_hides_redacted() {
    let s = NamedMixed {
        public: "hello".into(),
        secret: "shhh".into(),
        visible: 42,
    };
    let out = format!("{s:?}");
    assert!(out.contains("hello"), "public field missing: {out}");
    assert!(out.contains("42"), "visible field missing: {out}");
    assert!(!out.contains("shhh"), "secret leaked: {out}");
    assert!(out.contains("[4 bytes]"), "expected length: {out}");
}

// -- Option<String> tests --

#[test]
fn option_some_shows_length() {
    let s = NamedWithOption {
        maybe_secret: Some("password".into()),
    };
    let out = format!("{s:?}");
    assert!(!out.contains("password"), "secret leaked: {out}");
    assert!(out.contains("Some([8 bytes])"), "expected Some(len): {out}");
}

#[test]
fn option_none_shows_none() {
    let s = NamedWithOption { maybe_secret: None };
    let out = format!("{s:?}");
    assert!(out.contains("None"), "expected None: {out}");
}

// -- Newtype tests --

#[test]
fn newtype_redacted_hides_bytes() {
    let s = RedactedNewtype([0xFF; 16]);
    let out = format!("{s:?}");
    assert!(!out.contains("255"), "raw bytes leaked: {out}");
    assert!(out.contains("[16 bytes]"), "expected length: {out}");
}

#[test]
fn newtype_transparent_shows_value() {
    let s = TransparentNewtype(99);
    let out = format!("{s:?}");
    assert!(out.contains("99"), "value missing: {out}");
}

// -- Real type tests --

#[test]
fn content_key_shows_length_not_bytes() {
    let key = generate_content_key(&mut OsRng);
    let out = format!("{key:?}");
    assert!(out.starts_with("ContentKey"), "expected type name: {out}");
    assert!(out.contains("[32 bytes]"), "expected length: {out}");
}

#[test]
fn encrypted_payload_uses_standard_debug() {
    // EncryptedPayload uses normal Debug (not RedactedDebug) because
    // ciphertext and nonces are not secret — they're sent to the PDS.
    let key = generate_content_key(&mut OsRng);
    let payload = encrypt_blob(&key, b"test", &mut OsRng).unwrap();
    let out = format!("{payload:?}");
    assert!(
        out.contains("EncryptedPayload"),
        "expected type name: {out}"
    );
    assert!(out.contains("nonce"), "expected nonce field: {out}");
    assert!(
        out.contains("ciphertext"),
        "expected ciphertext field: {out}"
    );
}
