// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;

/// Params for the `fs/read_text_file` request sent to the ACP client.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsReadTextFileParams {
    pub session_id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}
