use std::io::Cursor;

use super::*;

fn reader(input: &str) -> Cursor<Vec<u8>> {
    Cursor::new(input.as_bytes().to_vec())
}

fn writer() -> Vec<u8> {
    Vec::new()
}

#[test]
fn confirm_yes() {
    let result = confirm_from("delete?", &mut reader("y\n"), &mut writer());
    assert!(result.unwrap());
}

#[test]
fn confirm_yes_uppercase() {
    let result = confirm_from("delete?", &mut reader("Y\n"), &mut writer());
    assert!(result.unwrap());
}

#[test]
fn confirm_no() {
    let result = confirm_from("delete?", &mut reader("n\n"), &mut writer());
    assert!(!result.unwrap());
}

#[test]
fn confirm_empty_is_no() {
    let result = confirm_from("delete?", &mut reader("\n"), &mut writer());
    assert!(!result.unwrap());
}

#[test]
fn confirm_garbage_is_no() {
    let result = confirm_from("delete?", &mut reader("sure\n"), &mut writer());
    assert!(!result.unwrap());
}

#[test]
fn confirm_prompt_format() {
    let mut output = writer();
    let _ = confirm_from("delete foo?", &mut reader("n\n"), &mut output);
    let prompt = String::from_utf8(output).unwrap();
    assert_eq!(prompt, "delete foo? [y/N] ");
}

#[test]
fn confirm_exact_match() {
    let result = confirm_exact_from(
        "WARNING: danger",
        "do it",
        &mut reader("do it\n"),
        &mut writer(),
        &mut writer(),
    );
    assert!(result.is_ok());
}

#[test]
fn confirm_exact_mismatch_bails() {
    let result = confirm_exact_from(
        "WARNING: danger",
        "do it",
        &mut reader("nope\n"),
        &mut writer(),
        &mut writer(),
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cancelled"));
}

#[test]
fn confirm_exact_empty_bails() {
    let result = confirm_exact_from(
        "WARNING",
        "confirm",
        &mut reader("\n"),
        &mut writer(),
        &mut writer(),
    );
    assert!(result.is_err());
}

#[test]
fn confirm_exact_shows_phrase_in_output() {
    let mut info = writer();
    let _ = confirm_exact_from(
        "WARNING: bad things",
        "I accept",
        &mut reader("I accept\n"),
        &mut info,
        &mut writer(),
    );
    let output = String::from_utf8(info).unwrap();
    assert!(output.contains("WARNING: bad things"));
    assert!(output.contains("Type exactly: I accept"));
}

#[test]
fn input_trims_whitespace() {
    let result = input_from("> ", &mut reader("  hello  \n"), &mut writer());
    assert_eq!(result.unwrap(), "hello");
}

#[test]
fn input_empty() {
    let result = input_from("> ", &mut reader("\n"), &mut writer());
    assert_eq!(result.unwrap(), "");
}

#[test]
fn input_with_default_empty_returns_default() {
    let result = input_with_default_from("path:", "/tmp/foo", &mut reader("\n"), &mut writer());
    assert_eq!(result.unwrap(), "/tmp/foo");
}

#[test]
fn input_with_default_nonempty_overrides() {
    let result = input_with_default_from(
        "path:",
        "/tmp/foo",
        &mut reader("/home/bar\n"),
        &mut writer(),
    );
    assert_eq!(result.unwrap(), "/home/bar");
}

#[test]
fn input_with_default_prompt_format() {
    let mut output = writer();
    let _ = input_with_default_from("save to:", "/tmp/x", &mut reader("\n"), &mut output);
    let prompt = String::from_utf8(output).unwrap();
    assert_eq!(prompt, "save to: [/tmp/x] ");
}
