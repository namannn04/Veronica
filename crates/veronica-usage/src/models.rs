//! `usage.json` schema version 8.
//!
//! These types mirror exactly what the bundled `refresh-usage` collector
//! writes, so Veronica reads the same document Edith does. Numeric fields are
//! all optional-with-default because the collector omits zero-valued keys in
//! several places, and a strict decode would reject real files.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The only schema this build understands.
pub const SCHEMA_VERSION: u32 = 8;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDocument {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub sources: Vec<String>,
    /// Sources shown when the user has not chosen a subset.
    #[serde(default)]
    pub default_sources: Vec<String>,
    #[serde(default)]
    pub source_meta: BTreeMap<String, SourceMeta>,
    #[serde(default)]
    pub totals: Totals,
    #[serde(default)]
    pub daily: Vec<DailyEntry>,
    #[serde(default)]
    pub sessions: Vec<SessionRef>,
    /// Present only when SSH machines have folded their usage in.
    #[serde(default)]
    pub machines: Vec<MachineRef>,
}

impl UsageDocument {
    /// Whether this document was written by a collector this build can read.
    /// A newer schema is refused rather than silently misread.
    pub fn is_compatible(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
    }

    /// Label for a source id, falling back to the id so an unknown collector
    /// still renders something meaningful.
    pub fn label_for(&self, source: &str) -> String {
        self.source_meta
            .get(source)
            .map(|meta| meta.label.clone())
            .unwrap_or_else(|| source.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMeta {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub tool: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyEntry {
    /// Local calendar day, `YYYY-MM-DD`.
    pub period: String,
    /// Source id to the per-model rows recorded for that source that day.
    #[serde(default)]
    pub by_source: BTreeMap<String, Vec<ModelRow>>,
    /// Always 24 buckets, midnight-first, in the machine's local time.
    #[serde(default)]
    pub hours: Vec<HourBucket>,
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRow {
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cost: f64,
}

impl ModelRow {
    pub fn tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HourBucket {
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub by_source: BTreeMap<String, Amount>,
    #[serde(default)]
    pub by_path: BTreeMap<String, Amount>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Amount {
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    #[serde(default)]
    pub project_name: String,
    // The collector spells these with acronym casing, which camelCase would
    // render as `repositoryId`/`repositoryUrl` and never match.
    #[serde(default, rename = "repositoryID")]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub repository_name: Option<String>,
    #[serde(default, rename = "repositoryURL")]
    pub repository_url: Option<String>,
    #[serde(default)]
    pub folder_name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub by_source: BTreeMap<String, SourceDetail>,
    #[serde(default)]
    pub chats: Vec<ChatEntry>,
    #[serde(default)]
    pub worktrees: Vec<WorktreeEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDetail {
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub by_model: BTreeMap<String, Amount>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost: f64,
    /// Milliseconds since the epoch, as the collector writes them.
    #[serde(default)]
    pub first_ts: Option<i64>,
    #[serde(default)]
    pub last_ts: Option<i64>,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub chats: Vec<ChatEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineRef {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub host: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_minimal_document_and_defaults_the_rest() {
        let doc: UsageDocument = serde_json::from_str(r#"{"schemaVersion":8}"#).unwrap();
        assert!(doc.is_compatible());
        assert_eq!(doc.totals, Totals::default());
        assert!(doc.daily.is_empty());
    }

    #[test]
    fn a_newer_schema_is_refused_rather_than_misread() {
        let doc: UsageDocument = serde_json::from_str(r#"{"schemaVersion":9}"#).unwrap();
        assert!(!doc.is_compatible());
    }

    #[test]
    fn model_row_tokens_sum_all_four_buckets() {
        let row = ModelRow {
            input_tokens: 1,
            output_tokens: 2,
            cache_creation_tokens: 4,
            cache_read_tokens: 8,
            ..Default::default()
        };
        assert_eq!(row.tokens(), 15);
    }

    #[test]
    fn unknown_source_falls_back_to_its_id() {
        let doc = UsageDocument::default();
        assert_eq!(doc.label_for("cli"), "cli");
    }

    #[test]
    fn decodes_the_real_shape_the_collector_emits() {
        let raw = r#"{
          "schemaVersion": 8,
          "generatedAt": "2026-08-20T00:32:00Z",
          "sources": ["cli"],
          "defaultSources": ["cli"],
          "sourceMeta": {"cli": {"label": "Claude Code", "tool": "Claude Code"}},
          "totals": {"cost": 1.5, "tokens": 100, "inputTokens": 10,
                     "outputTokens": 20, "cacheCreationTokens": 30, "cacheReadTokens": 40},
          "daily": [{
            "period": "2026-08-20",
            "bySource": {"cli": [{"modelName": "claude-opus-5", "inputTokens": 44,
                                  "outputTokens": 20373, "cacheCreationTokens": 71000,
                                  "cacheReadTokens": 1473816, "cost": 1.956453}]},
            "hours": [],
            "projects": [{
              "projectName": "edith", "repositoryID": "github.com/x/edith",
              "folderName": "edith", "path": "/home/u/edith",
              "tokens": 1339653, "cost": 1.67,
              "bySource": {"cli": {"tokens": 1339653, "cost": 1.67,
                                   "byModel": {"claude-opus-5": {"tokens": 1339653, "cost": 1.67}}}},
              "chats": [{"id": "abc", "path": "/home/u/edith", "title": "t",
                         "tokens": 1, "cost": 0.1, "firstTs": 1787165247000,
                         "lastTs": 1787166133000, "source": "cli"}],
              "worktrees": []
            }]
          }],
          "sessions": [{"id": "abc", "source": "cli"}]
        }"#;
        let doc: UsageDocument = serde_json::from_str(raw).unwrap();
        assert!(doc.is_compatible());
        assert_eq!(doc.label_for("cli"), "Claude Code");
        assert_eq!(doc.daily.len(), 1);
        let project = &doc.daily[0].projects[0];
        assert_eq!(project.repository_id.as_deref(), Some("github.com/x/edith"));
        assert_eq!(project.chats[0].first_ts, Some(1787165247000));
        assert_eq!(doc.daily[0].by_source["cli"][0].tokens(), 1_565_233);
    }
}
