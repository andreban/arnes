// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use super::*;

#[test]
fn test_tail_keeps_short_text() -> Result<()> {
    // -- Exec
    let (text, cut) = tail("hello", 5);

    // -- Check
    assert_eq!(text, "hello");
    assert!(!cut);

    Ok(())
}

#[test]
fn test_tail_keeps_most_recent_bytes() -> Result<()> {
    // -- Exec
    let (text, cut) = tail("first\nlast\n", 5);

    // -- Check
    assert_eq!(text, "last\n");
    assert!(cut);

    Ok(())
}

#[test]
fn test_tail_starts_on_char_boundary() -> Result<()> {
    // -- Exec
    // "é" is two bytes and "€" three; a 4-byte tail of "aé€" would start
    // inside "é", so it moves forward to "€".
    let (text, cut) = tail("aé€", 4);

    // -- Check
    assert_eq!(text, "€");
    assert!(cut);

    Ok(())
}
