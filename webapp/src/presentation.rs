//! Serializable contract between analysis providers and the Dioxus UI.
//!
//! Keep benchmark-specific types behind a provider. The browser only sees
//! descriptors and presentation data, never stored output payloads.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog<E> {
    pub experiments: Vec<ExperimentOption<E>>,
    pub analyses: Vec<AnalysisDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentOption<E> {
    pub id: E,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisDescriptor {
    pub id: String,
    pub label: String,
    pub description: String,
    pub parameters: Vec<ParameterDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterDescriptor {
    pub id: String,
    pub label: String,
    pub kind: ParameterKind,
    pub options: Vec<ParameterOption>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterKind {
    Select,
    MultiSelect,
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisRequest<E> {
    pub experiment: E,
    pub analysis: String,
    pub parameters: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub title: String,
    pub description: String,
    pub chart: ChartSpec,
    pub tables: Vec<TableSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub x_label: String,
    pub y_label: String,
    pub reference_lines: Vec<ReferenceLine>,
    pub series: Vec<ChartSeries>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceLine {
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSeries {
    pub id: String,
    pub label: String,
    pub points: Vec<ChartPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartPoint {
    pub x: String,
    pub y: Option<f64>,
    pub details: Vec<DetailField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSpec {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
