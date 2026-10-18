const hostId = "analysis-plot";
const host = document.getElementById(hostId);
const colors = ["#4b72b8", "#d6368f", "#31508a", "#9c286a", "#819ed0", "#ea91c2"];
const theme = getComputedStyle(document.documentElement);

function makeTraces(spec) {
  return spec.series.map((series, index) => ({
    uid: series.id,
    name: series.label,
    type: "scatter",
    mode: "lines+markers",
    connectgaps: false,
    x: series.points.map(point => point.x),
    y: series.points.map(point => point.y),
    marker: { color: colors[index % colors.length], size: 8 },
    line: { color: colors[index % colors.length], width: 2 },
    hovertemplate: "%{x}<br>%{y}<extra>%{fullData.name}</extra>"
  }));
}

function makeLayout(spec) {
  return {
    autosize: true,
    paper_bgcolor: "#ffffff",
    plot_bgcolor: "#ffffff",
    font: { family: theme.fontFamily, color: theme.getPropertyValue("--ink").trim() },
    margin: { l: 72, r: 24, t: 18, b: 95 },
    hovermode: "closest",
    dragmode: "pan",
    xaxis: {
      title: spec.x_label,
      type: "category",
      gridcolor: theme.getPropertyValue("--grid").trim(),
      tickangle: -35,
      automargin: true
    },
    yaxis: {
      title: spec.y_label,
      gridcolor: theme.getPropertyValue("--grid").trim(),
      zerolinecolor: theme.getPropertyValue("--grid").trim(),
      automargin: true
    },
    shapes: spec.reference_lines.map(line => ({
      type: "line", xref: "paper", x0: 0, x1: 1,
      y0: line.value, y1: line.value,
      line: { color: theme.getPropertyValue("--pink").trim(), dash: "dash", width: 1 }
    })),
    annotations: spec.reference_lines.map(line => ({
      xref: "paper", x: 1, y: line.value, text: line.label,
      showarrow: false, xanchor: "right", yshift: 10,
      font: { color: theme.getPropertyValue("--pink").trim(), size: 11 }
    }))
  };
}

let spec = await dioxus.recv();
while (true) {
  await Plotly.react(host, makeTraces(spec), makeLayout(spec), {
    responsive: true,
    scrollZoom: true,
    displaylogo: false,
    modeBarButtonsToRemove: ["lasso2d", "select2d"]
  });
  host.removeAllListeners("plotly_click");
  host.on("plotly_click", event => {
    const point = event.points[0];
    dioxus.send({ series_id: point.data.uid, point_index: point.pointIndex });
  });
  spec = await dioxus.recv();
}
