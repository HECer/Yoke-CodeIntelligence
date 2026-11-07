//! Selected count-only tool statistics. This never changes source or vendor billing.
use crate::{capture::Deadline, model};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use zeroize::Zeroizing;

pub mod http;
pub const VERSION: &str = "host_tool_usage_estimators_v1";
pub const MAX_TEXT_BYTES: usize = 4_000_000;
const MAX_TOOLS: usize = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Estimator {
    #[serde(rename = "CHAR_COUNT")]
    CharCount,
    #[serde(rename = "TIKTOKEN_GPT4O")]
    TiktokenGpt4o,
    #[serde(rename = "ANTHROPIC_CLAUDE_SONNET_4")]
    AnthropicClaudeSonnet4,
}
impl Estimator {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::CharCount => "CHAR_COUNT",
            Self::TiktokenGpt4o => "TIKTOKEN_GPT4O",
            Self::AnthropicClaudeSonnet4 => "ANTHROPIC_CLAUDE_SONNET_4",
        }
    }
}
pub(crate) fn binding() -> &'static str {
    static VALUE: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        model::identity(&(
            VERSION,
            model::hash(include_bytes!("tool_usage_contract.json")),
            "tiktoken-rs=0.12.0",
        ))
    });
    &VALUE
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    num_times_called: u64,
    input_tokens: u64,
    output_tokens: u64,
}
impl Entry {
    fn add(&self, input: u64, output: u64) -> Result<Self> {
        Ok(Self {
            num_times_called: self
                .num_times_called
                .checked_add(1)
                .context("tool_usage_counter_overflow")?,
            input_tokens: self
                .input_tokens
                .checked_add(input)
                .context("tool_usage_counter_overflow")?,
            output_tokens: self
                .output_tokens
                .checked_add(output)
                .context("tool_usage_counter_overflow")?,
        })
    }
}
enum Counter {
    Characters,
    Local(Arc<tiktoken_rs::CoreBPE>),
    Remote(http::Host),
}
fn local_tokenizer() -> Result<Arc<tiktoken_rs::CoreBPE>> {
    static TOKENIZER: std::sync::LazyLock<std::result::Result<Arc<tiktoken_rs::CoreBPE>, ()>> =
        std::sync::LazyLock::new(|| tiktoken_rs::o200k_base().map(Arc::new).map_err(|_| ()));
    TOKENIZER
        .as_ref()
        .map(Arc::clone)
        .map_err(|_| anyhow::anyhow!("tool_usage_local_tokenizer_unavailable"))
}
pub(crate) struct Stats {
    estimator: Estimator,
    counter: Counter,
    entries: BTreeMap<String, Entry>,
}
pub(crate) struct Measured {
    name: String,
    input: u64,
    output: u64,
}
fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_'),
        "tool_usage_name_invalid"
    );
    Ok(())
}
fn text(value: &Value, deadline: &Deadline) -> Result<Zeroizing<String>> {
    let raw = crate::json::encode_bounded(value, MAX_TEXT_BYTES, deadline)?;
    Ok(Zeroizing::new(
        String::from_utf8(raw).context("tool_usage_json_utf8")?,
    ))
}
impl Stats {
    pub(crate) fn annotate_native_result(&self, result: &mut Value) -> bool {
        if self.estimator != Estimator::AnthropicClaudeSonnet4
            || result["schemaVersion"] != 1
            || !matches!(result["status"].as_str(), Some("ok" | "partial"))
            || !result["metrics"].is_object()
        {
            return false;
        }
        let metrics = result["metrics"]
            .as_object_mut()
            .expect("Native metrics object");
        metrics.insert("localOnly".into(), json!(false));
        metrics.insert("externalUsage".into(), json!("unknown"));
        metrics.insert("toolTokenCounting".into(), json!({"requests":2,"generationCalls":0,"providerBillingEstablished":false,
            "nativeSizeMetricsScope":"underlying_native_result_before_tool_token_counting_metadata"}));
        true
    }
    pub(crate) fn new(
        estimator: Estimator,
        host: Option<&http::Host>,
        deadline: &Deadline,
    ) -> Result<Self> {
        deadline.check()?;
        let counter = match estimator {
            Estimator::CharCount => Counter::Characters,
            Estimator::TiktokenGpt4o => {
                // Compiled MIT vocabulary only: no filesystem cache or runtime download.
                Counter::Local(local_tokenizer()?)
            }
            Estimator::AnthropicClaudeSonnet4 => {
                let host = host.context("tool_usage_independent_text_disclosure_grant_required")?;
                host.check(deadline)?;
                Counter::Remote(host.clone())
            }
        };
        deadline.check()?;
        Ok(Self {
            estimator,
            counter,
            entries: BTreeMap::new(),
        })
    }
    fn check(&self, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        if let Counter::Remote(host) = &self.counter {
            host.check(deadline)?;
        }
        Ok(())
    }
    #[cfg(test)]
    fn count(&self, value: &str, deadline: &Deadline) -> Result<u64> {
        self.count_checked(value, deadline, &|| Ok(()))
    }
    fn count_checked(
        &self,
        value: &str,
        deadline: &Deadline,
        current: &dyn Fn() -> Result<()>,
    ) -> Result<u64> {
        self.check(deadline)?;
        current()?;
        ensure!(value.len() <= MAX_TEXT_BYTES, "tool_usage_text_capacity");
        let count = match &self.counter {
            Counter::Characters => {
                let mut characters = 0u64;
                for (n, _) in value.chars().enumerate() {
                    if n % 4096 == 0 {
                        deadline.check()?;
                    }
                    characters += 1;
                }
                characters / 4
            }
            Counter::Local(bpe) => {
                let tokens = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    bpe.encode_ordinary(value)
                }))
                .map_err(|_| anyhow::anyhow!("tool_usage_local_tokenization_failed"))?;
                u64::try_from(tokens.len()).context("tool_usage_counter_overflow")?
            }
            Counter::Remote(host) => host.count_checked(value, deadline, current)?,
        };
        self.check(deadline)?;
        current()?;
        Ok(count)
    }
    pub(crate) fn preflight(&self, tool: &str, args: &Value, deadline: &Deadline) -> Result<()> {
        self.check(deadline)?;
        name(tool)?;
        ensure!(
            self.entries.contains_key(tool) || self.entries.len() < MAX_TOOLS,
            "tool_usage_tool_capacity"
        );
        let _ = text(args, deadline)?;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn measure(
        &self,
        tool: &str,
        args: &Value,
        result: &Value,
        deadline: &Deadline,
    ) -> Result<Measured> {
        self.measure_checked(tool, args, result, deadline, &|| Ok(()))
    }
    pub(crate) fn measure_checked(
        &self,
        tool: &str,
        args: &Value,
        result: &Value,
        deadline: &Deadline,
        current: &dyn Fn() -> Result<()>,
    ) -> Result<Measured> {
        self.preflight(tool, args, deadline)?;
        // Admit both complete representations before any external disclosure.
        let input = text(args, deadline)?;
        let output = text(result, deadline)?;
        current()?;
        let input = self.count_checked(&input, deadline, current)?;
        let output = self.count_checked(&output, deadline, current)?;
        Ok(Measured {
            name: tool.to_owned(),
            input,
            output,
        })
    }
    pub(crate) fn commit(&mut self, measured: Measured, deadline: &Deadline) -> Result<()> {
        self.check(deadline)?;
        ensure!(
            self.entries.contains_key(&measured.name) || self.entries.len() < MAX_TOOLS,
            "tool_usage_tool_capacity"
        );
        let next = self
            .entries
            .get(&measured.name)
            .cloned()
            .unwrap_or_default()
            .add(measured.input, measured.output)?;
        self.entries.insert(measured.name, next);
        Ok(())
    }
    pub(crate) fn view(&self) -> Value {
        json!({"estimator":self.estimator.name(),"tools":self.entries,"scope":"current_session_instance_successful_serena_operations",
            "inputRepresentation":"canonical_json_arguments","outputRepresentation":"canonical_json_native_tool_result",
            "textStored":false,"nativeByteEstimateChanged":false,"providerBillingEstablished":false,
            "hostGrantRemoteAttempts":match &self.counter { Counter::Remote(host) => host.attempts(), _ => 0 },
            "upstreamTextRepresentationIdentical":false})
    }
    pub(crate) fn clear(&mut self, deadline: &Deadline) -> Result<()> {
        self.check(deadline)?;
        self.entries.clear();
        Ok(())
    }
    pub(crate) fn dashboard_view(&self, revision: u64) -> Value {
        let tools = self
            .entries
            .iter()
            .map(|(name, entry)| {
                (
                    name.clone(),
                    json!({
                        "numTimesCalled":entry.num_times_called.to_string(),
                        "inputTokens":entry.input_tokens.to_string(),
                        "outputTokens":entry.output_tokens.to_string()
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>();
        json!({"schema":"native_tool_usage_display_v1","estimator":self.estimator.name(),
            "scope":"current_session_instance_successful_serena_operations","revision":revision.to_string(),
            "hostGrantRemoteAttempts":match &self.counter { Counter::Remote(host) => host.attempts(), _ => 0 }.to_string(),
            "tools":tools})
    }
}
#[cfg(test)]
#[path = "tool_usage_dashboard_tests.rs"]
mod dashboard_tests;
#[cfg(test)]
#[path = "tool_usage_tests.rs"]
mod tests;
