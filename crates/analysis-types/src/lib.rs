//! Shared analysis plans, capabilities, and results for native and browser clients.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog<E> {
    pub experiments: Vec<ExperimentOption<E>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentOption<E> {
    pub id: E,
    pub label: String,
    pub analyses: Vec<AnalysisDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisDescriptor {
    pub id: String,
    pub label: String,
    pub description: String,
    pub metrics: Vec<MetricDescriptor>,
    pub benchmarks: Vec<BenchmarkOption>,
    pub commits: Vec<CommitOption>,
    pub filter_fields: Vec<FilterField>,
    /// Optional controls for operations specific to a registered analysis.
    pub parameters: Vec<ParameterDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricDescriptor {
    pub id: String,
    pub label: String,
    pub description: String,
    pub unit: String,
    pub formula: MetricFormula,
    pub repetition_reductions: Vec<Reduction>,
    pub benchmark_reductions: Vec<Reduction>,
    pub default_repetitions: Reduction,
    pub default_benchmarks: Reduction,
    pub reference: Option<ReferenceLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricFormula {
    Identity,
    Ratio,
    PercentSavings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reduction {
    Mean,
    Median,
    Min,
    Max,
    Sum,
    Count,
    StdDev,
    P90,
    P95,
    RatioOfMeans,
}

impl Reduction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mean => "Mean",
            Self::Median => "Median",
            Self::Min => "Minimum",
            Self::Max => "Maximum",
            Self::Sum => "Sum",
            Self::Count => "Count",
            Self::StdDev => "Standard deviation",
            Self::P90 => "90th percentile",
            Self::P95 => "95th percentile",
            Self::RatioOfMeans => "Ratio of means",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkOption {
    pub id: usize,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisPlan {
    pub metric: String,
    pub selection: Selection,
    pub repetitions: Reduction,
    pub benchmarks: Reduction,
    pub series: SeriesGrouping,
    pub filters: Vec<FilterClause>,
    pub parameters: Map<String, Value>,
}

impl AnalysisPlan {
    pub fn for_descriptor(descriptor: &AnalysisDescriptor) -> Self {
        let metric = descriptor.metrics.first();
        Self {
            metric: metric.map(|m| m.id.clone()).unwrap_or_default(),
            selection: Selection::default(),
            repetitions: metric.map(|m| m.default_repetitions).unwrap_or(Reduction::Mean),
            benchmarks: metric.map(|m| m.default_benchmarks).unwrap_or(Reduction::Mean),
            series: SeriesGrouping::Combined,
            filters: Vec::new(),
            parameters: descriptor.parameters.iter().filter_map(|p| {
                p.default.as_ref().map(|v| (p.id.clone(), Value::String(v.clone())))
            }).collect(),
        }
    }

    pub fn select_metric(&mut self, metric: &MetricDescriptor) {
        self.metric = metric.id.clone();
        self.repetitions = metric.default_repetitions;
        self.benchmarks = metric.default_benchmarks;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    /// None selects all; Some([]) deliberately selects none.
    pub benchmarks: Option<Vec<usize>>,
    pub commits: CommitSelection,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CommitSelection {
    #[default]
    All,
    Range { start: Option<String>, end: Option<String> },
    Only { ids: Vec<String> },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "field", rename_all = "snake_case")]
pub enum SeriesGrouping {
    #[default]
    Combined,
    Benchmark,
    Parameter(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterField {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind { Number, Text, Boolean }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterClause {
    pub field: String,
    pub operator: FilterOperator,
    pub value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator { Equal, NotEqual, Greater, AtLeast, Less, AtMost, Contains, Present, Missing }

impl FilterOperator {
    pub fn label(self) -> &'static str {
        match self {
            Self::Equal => "equals", Self::NotEqual => "does not equal",
            Self::Greater => "greater than", Self::AtLeast => "at least",
            Self::Less => "less than", Self::AtMost => "at most",
            Self::Contains => "contains", Self::Present => "is present", Self::Missing => "is missing",
        }
    }
    pub fn for_kind(kind: FieldKind) -> Vec<Self> {
        let mut operators = vec![Self::Equal, Self::NotEqual];
        match kind {
            FieldKind::Number => operators.extend([Self::Greater, Self::AtLeast, Self::Less, Self::AtMost]),
            FieldKind::Text => operators.push(Self::Contains),
            FieldKind::Boolean => (),
        }
        operators.extend([Self::Present, Self::Missing]);
        operators
    }
}

impl FilterField {
    pub fn clause(&self) -> FilterClause {
        FilterClause {
            field: self.id.clone(),
            operator: FilterOperator::Equal,
            value: self.choices.first().cloned().map(Value::String).unwrap_or_else(|| match self.kind {
                FieldKind::Number => Value::from(0), FieldKind::Text => Value::String(String::new()),
                FieldKind::Boolean => Value::Bool(true),
            }),
        }
    }
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
pub enum ParameterKind { Select, MultiSelect, Text }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterOption { pub value: String, pub label: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisRequest<E> {
    pub experiment: E,
    pub analysis: String,
    pub plan: AnalysisPlan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub title: String,
    pub description: String,
    pub chart: ChartSpec,
    pub coverage: CoverageSummary,
    pub tables: Vec<TableSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverageSummary { pub included: usize, pub selected: usize }

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PointCoverage {
    pub included: usize,
    pub selected: usize,
    pub exclusions: Vec<Exclusion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exclusion { pub benchmark: usize, pub reason: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub x_label: String,
    pub y_label: String,
    pub reference_lines: Vec<ReferenceLine>,
    pub series: Vec<ChartSeries>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceLine { pub value: f64, pub label: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSeries { pub id: String, pub label: String, pub points: Vec<ChartPoint> }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartPoint {
    pub x: String,
    pub y: Option<f64>,
    pub coverage: PointCoverage,
    pub details: Vec<DetailField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailField { pub label: String, pub value: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSpec { pub title: String, pub columns: Vec<String>, pub rows: Vec<Vec<String>> }

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisScale { #[default] Linear, Log }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlotOptions {
    pub y_scale: AxisScale,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub show_reference_lines: bool,
}

impl Default for PlotOptions {
    fn default() -> Self {
        Self { y_scale: AxisScale::Linear, y_min: None, y_max: None, show_reference_lines: true }
    }
}

impl PlotOptions {
    pub fn bounds_error(&self) -> Option<&'static str> {
        if self.y_min.into_iter().chain(self.y_max).any(|v| !v.is_finite()) {
            Some("Axis bounds must be finite.")
        } else if self.y_scale == AxisScale::Log && self.y_min.into_iter().chain(self.y_max).any(|v| v <= 0.0) {
            Some("Logarithmic axis bounds must be positive.")
        } else if self.y_min.zip(self.y_max).is_some_and(|(min, max)| min >= max) {
            Some("The minimum must be below the maximum.")
        } else { None }
    }
}
