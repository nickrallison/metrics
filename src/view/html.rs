//! Portable graph snapshots. Projection math stays in the domain's `View`.

use std::{collections::BTreeMap, time::SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;

use super::Series;
use crate::Result;

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub experiment: i64,
    pub generated_at: String,
    pub commits: Vec<SnapshotCommit>,
    pub workloads: Vec<Workload>,
    pub metrics: Vec<Metric>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotCommit {
    pub hash: String,
    pub author: String,
    pub message: String,
    pub committed_at: String,
}

#[derive(Debug, Serialize)]
pub struct Workload {
    pub index: usize,
    pub label: String,
    pub parameters: Value,
}

#[derive(Debug, Serialize)]
pub struct Metric {
    pub id: String,
    pub description: String,
    pub reference: f64,
    pub reference_label: String,
    pub aggregate: Series,
    pub inputs: BTreeMap<usize, Series>,
}

pub(crate) fn timestamp(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Secs, true)
}

impl Snapshot {
    pub fn render(&self) -> Result<String> {
        // JSON is embedded as script text, whose HTML parser recognizes closing
        // tags even inside strings. Never interpolate user data into HTML/JS.
        let data = serde_json::to_string(self)?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        let mut html = include_str!("../../ui/results.html")
            .replace("/* RESULTS_CSS */", include_str!("../../ui/results.css"))
            .replace(
                "/* PLOTLY_LICENSE */",
                concat!(
                    "/*\n",
                    include_str!("../../ui/vendor/plotly.LICENSE"),
                    "\n*/"
                ),
            )
            .replace(
                "/* PLOTLY_JS */",
                include_str!("../../ui/vendor/plotly-basic.min.js"),
            )
            .replace("/* RESULTS_JS */", include_str!("../../ui/results.js"));
        html = html.replace("<!-- SNAPSHOT_DATA -->", &data);
        Ok(html)
    }
}
