// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{Micros, ModelCostInfo, ModelKey, ModelPricing, TokenCounts};

fn key(provider: &str, model_id: &str) -> ModelKey {
    ModelKey {
        provider: provider.into(),
        model_id: model_id.into(),
    }
}

fn pricing(input: i64, output: i64) -> ModelPricing {
    ModelPricing {
        input_per_mtok: Micros(input),
        output_per_mtok: Micros(output),
        cache_read_per_mtok: None,
        cache_write_per_mtok: None,
        thinking_per_mtok: None,
    }
}

fn block(
    provider: &str,
    default: ModelCostInfo,
    models: &[(&str, ModelPricing)],
) -> ProviderPricing {
    ProviderPricing {
        provider: provider.into(),
        default,
        models: models
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    }
}

fn one_mtok_each() -> TokenCounts {
    TokenCounts {
        input: 1_000_000,
        output: 1_000_000,
        cache_read: 0,
        cache_write: 0,
        thinking: 0,
    }
}

#[test]
fn per_model_match() {
    let reg = PricingRegistry::from_providers([block(
        "gemini",
        ModelCostInfo::Unknown,
        &[("gemini-2.5-flash", pricing(75, 300))],
    )]);
    assert!(matches!(
        reg.lookup(&key("gemini", "gemini-2.5-flash")),
        ModelCostInfo::Priced(_)
    ));
    assert!(matches!(
        reg.lookup(&key("gemini", "gemini-2.5-pro")),
        ModelCostInfo::Unknown
    ));
}

#[test]
fn provider_default_matches_any_model_id() {
    let reg = PricingRegistry::from_providers([block("ollama", ModelCostInfo::Free, &[])]);
    assert!(matches!(
        reg.lookup(&key("ollama", "llama3.2")),
        ModelCostInfo::Free
    ));
    assert!(matches!(
        reg.lookup(&key("ollama", "some-custom-finetune")),
        ModelCostInfo::Free
    ));
}

#[test]
fn per_model_beats_provider_default() {
    let reg = PricingRegistry::from_providers([block(
        "ollama",
        ModelCostInfo::Free,
        &[("billed-model", pricing(10, 20))],
    )]);
    assert!(matches!(
        reg.lookup(&key("ollama", "billed-model")),
        ModelCostInfo::Priced(_)
    ));
    assert!(matches!(
        reg.lookup(&key("ollama", "anything-else")),
        ModelCostInfo::Free
    ));
}

#[test]
fn unmatched_provider_returns_unknown() {
    let reg = PricingRegistry::default();
    assert!(matches!(
        reg.lookup(&key("nobody", "nothing")),
        ModelCostInfo::Unknown
    ));
}

#[test]
fn later_provider_block_replaces_earlier() {
    let reg = PricingRegistry::from_providers([
        block("gemini", ModelCostInfo::Free, &[("flash", pricing(10, 20))]),
        block(
            "gemini",
            ModelCostInfo::Unknown,
            &[("pro", pricing(75, 300))],
        ),
    ]);
    assert!(matches!(
        reg.lookup(&key("gemini", "pro")),
        ModelCostInfo::Priced(_)
    ));
    // The first block's `flash` and `Free` default are gone — the
    // second block replaced the first atomically.
    assert!(matches!(
        reg.lookup(&key("gemini", "flash")),
        ModelCostInfo::Unknown
    ));
    assert!(matches!(
        reg.lookup(&key("gemini", "anything-else")),
        ModelCostInfo::Unknown
    ));
}

#[test]
fn compute_cost_three_state() {
    let reg = PricingRegistry::from_providers([
        block(
            "gemini",
            ModelCostInfo::Unknown,
            &[("flash", pricing(75, 300))],
        ),
        block("local", ModelCostInfo::Free, &[]),
    ]);

    let Some(c) = reg.compute_cost(&key("gemini", "flash"), &one_mtok_each()) else {
        panic!("expected Some for Priced");
    };
    assert_eq!(c.total, Micros(75 + 300));

    let Some(c) = reg.compute_cost(&key("local", "self-hosted"), &one_mtok_each()) else {
        panic!("expected Some for Free");
    };
    assert_eq!(c.input, Micros::ZERO);
    assert_eq!(c.output, Micros::ZERO);
    assert_eq!(c.total, Micros::ZERO);

    assert!(
        reg.compute_cost(&key("unknown", "model"), &one_mtok_each())
            .is_none()
    );
}

#[test]
fn parse_well_formed_file() {
    let src = r#"
    [
      {
        "provider": "gemini",
        "models": {
          "gemini-2.5-flash": { "input_per_mtok": 0.075, "output_per_mtok": 0.30 },
          "gemini-2.5-pro":   { "input_per_mtok": 1.25,  "output_per_mtok": 5.0  }
        }
      },
      { "provider": "ollama", "default": "free" }
    ]
    "#;
    let reg = PricingRegistry::from_providers(parse_pricing(src).expect("parse"));
    assert!(matches!(
        reg.lookup(&key("gemini", "gemini-2.5-flash")),
        ModelCostInfo::Priced(_)
    ));
    assert!(matches!(
        reg.lookup(&key("gemini", "gemini-2.5-pro")),
        ModelCostInfo::Priced(_)
    ));
    assert!(matches!(
        reg.lookup(&key("ollama", "llama3.2")),
        ModelCostInfo::Free
    ));
}

#[test]
fn parse_provider_default_only() {
    let src = r#"[{ "provider": "ollama", "default": "free" }]"#;
    let reg = PricingRegistry::from_providers(parse_pricing(src).expect("parse"));
    assert!(matches!(
        reg.lookup(&key("ollama", "anything")),
        ModelCostInfo::Free
    ));
}

#[test]
fn parse_models_only_falls_through_to_unknown() {
    let src = r#"[{
        "provider": "gemini",
        "models": { "flash": { "input_per_mtok": 0.075, "output_per_mtok": 0.30 } }
    }]"#;
    let reg = PricingRegistry::from_providers(parse_pricing(src).expect("parse"));
    assert!(matches!(
        reg.lookup(&key("gemini", "flash")),
        ModelCostInfo::Priced(_)
    ));
    assert!(matches!(
        reg.lookup(&key("gemini", "pro")),
        ModelCostInfo::Unknown
    ));
}

#[test]
fn parse_malformed_json_errs() {
    assert!(parse_pricing("not json {{").is_err());
}

#[test]
fn parse_model_missing_required_rate_errs() {
    let src = r#"[{
        "provider": "gemini",
        "models": { "flash": { "input_per_mtok": 0.075 } }
    }]"#;
    assert!(parse_pricing(src).is_err());
}

#[test]
fn parse_converts_f64_dollars_to_micros() {
    let src = r#"[{
        "provider": "gemini",
        "models": { "flash": { "input_per_mtok": 0.075, "output_per_mtok": 0.30 } }
    }]"#;
    let reg = PricingRegistry::from_providers(parse_pricing(src).expect("parse"));
    let ModelCostInfo::Priced(p) = reg.lookup(&key("gemini", "flash")) else {
        panic!("expected Priced");
    };
    assert_eq!(p.input_per_mtok, Micros(75_000));
    assert_eq!(p.output_per_mtok, Micros(300_000));
}

#[test]
fn parse_self_hosted_paid_provider_default() {
    let src = r#"[{
        "provider": "self-hosted",
        "default": { "priced": { "input_per_mtok": 0.05, "output_per_mtok": 0.20 } }
    }]"#;
    let reg = PricingRegistry::from_providers(parse_pricing(src).expect("parse"));
    let ModelCostInfo::Priced(p) = reg.lookup(&key("self-hosted", "anything")) else {
        panic!("expected Priced");
    };
    assert_eq!(p.input_per_mtok, Micros(50_000));
    assert_eq!(p.output_per_mtok, Micros(200_000));
}
