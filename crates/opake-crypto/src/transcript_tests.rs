use super::{context_transcript, SEAL_AAD_LABEL, WRAP_INFO_LABEL};

#[test]
fn layout_is_label_count_then_length_prefixed_fields() {
    let out = context_transcript(b"lbl", &[b"ab", b"c"]);
    let expected = [
        b"lbl".as_slice(),
        &2u32.to_le_bytes(),
        &2u32.to_le_bytes(),
        b"ab",
        &1u32.to_le_bytes(),
        b"c",
    ]
    .concat();
    assert_eq!(out, expected);
}

// spec: document-crypto § Wraps are AEAD-bound to their record context
// (scenario: delimiter-straddling field pairs derive different keys)
#[test]
fn delimiter_straddling_field_pairs_encode_differently() {
    // Under hyphen-joining both tuples flatten to "…x-a-b".
    let a = context_transcript(WRAP_INFO_LABEL, &[b"at://x-a", b"b"]);
    let b = context_transcript(WRAP_INFO_LABEL, &[b"at://x", b"a-b"]);
    assert_ne!(a, b);
}

#[test]
fn field_boundary_shifts_encode_differently() {
    assert_ne!(
        context_transcript(b"l", &[b"ab", b""]),
        context_transcript(b"l", &[b"a", b"b"]),
    );
    assert_ne!(
        context_transcript(b"l", &[b"ab"]),
        context_transcript(b"l", &[b"a", b"b"]),
    );
}

#[test]
fn empty_fields_are_preserved_positionally() {
    assert_ne!(
        context_transcript(b"l", &[b"", b"x"]),
        context_transcript(b"l", &[b"x", b""]),
    );
}

#[test]
fn consumer_labels_domain_separate() {
    let fields: &[&[u8]] = &[b"same", b"fields"];
    assert_ne!(
        context_transcript(WRAP_INFO_LABEL, fields),
        context_transcript(SEAL_AAD_LABEL, fields),
    );
}
