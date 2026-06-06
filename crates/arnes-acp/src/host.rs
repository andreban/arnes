// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use arnes_core::Host;
use async_trait::async_trait;

/// ACP host stub. All operations return Unsupported until M2+ tools land.
pub struct AcpHost;

#[async_trait]
impl Host for AcpHost {}
