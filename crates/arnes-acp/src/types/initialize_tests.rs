// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

type Result<T, E = Box<dyn std::error::Error>> = core::result::Result<T, E>;

use super::*;
use serde_json::json;

#[test]
fn missing_client_capabilities_default_to_false() -> Result<()> {
    let params: InitializeParams = serde_json::from_value(json!({ "protocolVersion": 1 }))?;

    let caps = params.client_capabilities;
    assert!(!caps.fs.read_text_file);
    assert!(!caps.fs.write_text_file);
    assert!(!caps.terminal);
    Ok(())
}

#[test]
fn partial_client_capabilities_default_omitted_fields() -> Result<()> {
    let params: InitializeParams = serde_json::from_value(json!({
        "protocolVersion": 1,
        "clientCapabilities": { "fs": { "readTextFile": true } }
    }))?;

    let caps = params.client_capabilities;
    assert!(caps.fs.read_text_file);
    assert!(!caps.fs.write_text_file);
    assert!(!caps.terminal);
    Ok(())
}

#[test]
fn full_client_capabilities_are_read() -> Result<()> {
    let params: InitializeParams = serde_json::from_value(json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": true, "writeTextFile": true },
            "terminal": true
        }
    }))?;

    let caps = params.client_capabilities;
    assert!(caps.fs.read_text_file);
    assert!(caps.fs.write_text_file);
    assert!(caps.terminal);
    Ok(())
}
