const host = document.getElementById("analysis-plot");
const colors = ["#4b72b8", "#d6368f", "#31508a", "#9c286a", "#819ed0", "#ea91c2"];
const theme = getComputedStyle(document.documentElement);

function makeTraces(chart, options) {
  return chart.series.map((series, index) => ({
    uid: series.id,
    name: series.label,
    type: "scatter",
    mode: "lines+markers",
    connectgaps: false,
    x: series.points.map(point => point.x),
    // Keep point indices and original Rust values intact when a log axis
    // cannot display a measurement. The Dioxus view reports these omissions.
    y: series.points.map(point => options.y_scale === "log" && point.y <= 0 ? null : point.y),
    marker: { color: colors[index % colors.length], size: 7 },
    line: { color: colors[index % colors.length], width: 2 },
    hovertemplate: "%{x}<br>%{y}<extra>%{fullData.name}</extra>"
  }));
}

function makeLayout(chart, options) {
  const commits = [...new Set(chart.series.flatMap(series => series.points.map(point => point.x)))];
  const references = options.show_reference_lines
    ? chart.reference_lines.filter(line => options.y_scale !== "log" || line.value > 0)
    : [];
  const bound = value => value == null ? null : options.y_scale === "log" ? Math.log10(value) : value;
  const range = options.y_min == null && options.y_max == null ? undefined : [bound(options.y_min), bound(options.y_max)];
  return {
    autosize: true,
    paper_bgcolor: "#ffffff",
    plot_bgcolor: "#ffffff",
    font: { family: theme.fontFamily, color: theme.getPropertyValue("--ink").trim() },
    margin: { l: 72, r: 24, t: 24, b: 80 },
    legend: { orientation: "h", y: -0.25 },
    hovermode: "closest",
    dragmode: "pan",
    xaxis: {
      title: { text: chart.x_label },
      type: "category",
      categoryorder: "array", categoryarray: commits,
      tickmode: "array", tickvals: commits, ticktext: commits.map(commit => commit.slice(0, 8)),
      gridcolor: theme.getPropertyValue("--grid").trim(),
      tickangle: -40, automargin: true
    },
    yaxis: {
      title: { text: chart.y_label },
      type: options.y_scale,
      range,
      autorange: range ? undefined : true,
      gridcolor: theme.getPropertyValue("--grid").trim(),
      zerolinecolor: theme.getPropertyValue("--grid").trim(),
      automargin: true
    },
    shapes: references.map(line => ({
      type: "line", xref: "paper", x0: 0, x1: 1,
      y0: line.value, y1: line.value,
      line: { color: theme.getPropertyValue("--pink").trim(), dash: "dash", width: 1 }
    })),
    annotations: references.map(line => ({
      xref: "paper", x: 1, y: bound(line.value), text: line.label,
      showarrow: false, xanchor: "right", yshift: 10,
      font: { color: theme.getPropertyValue("--pink").trim(), size: 11 }
    }))
  };
}

let spec = await dioxus.recv();
while (spec) {
  if (spec.chart) {
    await Plotly.react(host, makeTraces(spec.chart, spec.options), makeLayout(spec.chart, spec.options), {
      responsive: true, scrollZoom: true, displaylogo: false,
      modeBarButtonsToRemove: ["lasso2d", "select2d"]
    });
    host.removeAllListeners("plotly_click");
    host.on("plotly_click", event => {
      const point = event.points[0];
      dioxus.send({ series_id: point.data.uid, point_index: point.pointIndex });
    });
  } else if (host.data) {
    Plotly.purge(host);
  }
  spec = await dioxus.recv();
}
Plotly.purge(host);
