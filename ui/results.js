/* No projection formulas here: every series is computed by McmView in Rust. */
"use strict";
(async () => {
  const $ = id => document.getElementById(id);
  const snapshot = JSON.parse($("snapshot-data").textContent);
  const plot = $("plot");
  const colors = ["#4b72b8", "#d6368f", "#31508a", "#9c286a", "#819ed0", "#ea91c2"];
  const symbols = ["circle", "diamond", "square", "triangle-up", "cross", "triangle-down"];
  const dashes = ["solid", "dash", "dot", "dashdot"];
  const chosen = new Set(snapshot.workloads.slice(0, 2).map(input => input.index));
  const workloads = new Map(snapshot.workloads.map(input => [input.index, input]));
  let pinned = null;
  let dragmode = "pan";
  let ready = false;
  // Serialize redraws so quick selector changes cannot race Plotly.react.
  let rendering = Promise.resolve();
  const node = (tag, text, className) => {
    const element = document.createElement(tag);
    if (text !== undefined) element.textContent = text;
    if (className) element.className = className;
    return element;
  };
  const option = (value, label) => {
    const element = node("option", label);
    element.value = value;
    element.title = label;
    return element;
  };
  // Plotly hover strings support markup, unlike the ordinary text-only DOM.
  const escape = text => String(text).replace(/[&<>"']/g, char => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
  })[char]);
  const subject = commit => commit.message.split("\n")[0];
  const format = (value, metric) => {
    if (value === null) return "No measurement";
    const number = Math.abs(value) >= 1e5 || (value !== 0 && Math.abs(value) < .001)
      ? value.toExponential(3) : value.toLocaleString("en", {maximumFractionDigits: 4});
    return `${number}${metric.aggregate.unit === "%" ? "%" : "×"}`;
  };
  const metric = () => snapshot.metrics.find(item => item.id === $("metric").value);
  const ticksInRange = range => {
    const first = Math.max(0, Math.ceil(range[0]));
    const last = Math.min(snapshot.commits.length - 1, Math.floor(range[1]));
    const step = Math.max(1, Math.ceil((last - first + 1) / Math.max(2, Math.floor((plot.clientWidth - 104) / 105))));
    return snapshot.commits.map((_, index) => index).filter(index => index >= first && index <= last && ((index - first) % step === 0 || index === last));
  };
  const tickText = ticks => ticks.map(index => snapshot.commits[index].hash.slice(0, 8));
  const projections = current => {
    const mode = $("projection").value;
    if (mode === "all") return [{id: "all", label: "All inputs", series: current.aggregate, style: 0}];
    const indices = mode === "single" ? [Number($("workload").value)] : [...chosen];
    return snapshot.workloads.filter(input => indices.includes(input.index)).map(input => ({
      id: `input-${input.index}`, label: input.label, series: current.inputs[input.index],
      style: snapshot.workloads.indexOf(input)
    }));
  };

  document.title = `MCM history · Experiment ${snapshot.experiment}`;
  $("experiment").textContent = `Experiment ${snapshot.experiment}`;
  $("snapshot-meta").textContent = `${snapshot.commits.length} commits · ${snapshot.workloads.length} inputs · Exported ${snapshot.generated_at}`;
  snapshot.metrics.forEach(item => $("metric").append(option(item.id, item.aggregate.label)));
  snapshot.workloads.forEach(input => {
    $("workload").append(option(input.index, input.label));
    const label = node("label", undefined, "input-choice");
    const checkbox = node("input");
    checkbox.type = "checkbox";
    checkbox.value = input.index;
    checkbox.checked = chosen.has(input.index);
    checkbox.addEventListener("change", () => {
      checkbox.checked ? chosen.add(input.index) : chosen.delete(input.index);
      redraw();
    });
    label.append(checkbox, node("span", input.label));
    $("compare-inputs").append(label);
    const details = node("div", undefined, "input-parameter");
    details.append(node("h3", input.label), node("pre", JSON.stringify(input.parameters, null, 2)));
    $("input-parameters").append(details);
  });
  snapshot.commits.forEach((commit, index) => {
    $("commit").append(option(index, `${commit.hash.slice(0, 8)} · ${subject(commit)}`));
  });

  function details() {
    if (pinned === null) return;
    const commit = snapshot.commits[pinned];
    const current = metric();
    $("details-title").textContent = subject(commit);
    $("commit").value = pinned;
    const container = $("point-details");
    container.replaceChildren();
    container.append(node("p", `${commit.hash} · ${commit.author} · ${commit.committed_at}`, "commit-meta"));
    const table = node("table", undefined, "results-table");
    const header = node("tr");
    ["Projection", current.aggregate.unit === "%" ? "Area savings" : "Runtime ratio", "Valid inputs"].forEach(text => header.append(node("th", text)));
    const head = node("thead"); head.append(header); table.append(head);
    const body = node("tbody");
    const exclusions = node("div");
    projections(current).forEach(item => {
      const point = item.series.points[pinned];
      const row = node("tr");
      row.append(node("td", item.label), node("td", format(point.value, current), "value"), node("td", `${point.coverage.included} / ${point.coverage.total}`));
      body.append(row);
      if (point.coverage.exclusions.length) {
        const disclosure = node("details", undefined, "exclusions");
        disclosure.open = point.value === null;
        disclosure.append(node("summary", `${item.label}: ${point.coverage.exclusions.length} excluded input(s)`));
        const list = node("ul");
        point.coverage.exclusions.forEach(excluded => {
          list.append(node("li", `${workloads.get(excluded.benchmark_index)?.label ?? `#${excluded.benchmark_index}`}: ${excluded.reason}`));
        });
        disclosure.append(list); exclusions.append(disclosure);
      }
    });
    table.append(body); container.append(table, exclusions);
    if (commit.message.includes("\n")) {
      const message = node("details");
      message.append(node("summary", "Full commit message"), node("pre", commit.message));
      container.append(message);
    }
  }

  function pin(index) {
    pinned = index;
    details();
    const numeric = plot.data.flatMap((trace, index) => trace.yaxis === "y2" ? [] : [index]);
    if (numeric.length) Plotly.restyle(plot, {selectedpoints: [[pinned]]}, numeric);
  }

  function redraw() {
    rendering = rendering.then(draw).catch(showError);
    return rendering;
  }

  function showError(error) {
    $("error").hidden = false;
    $("error").textContent = `Unable to render graph: ${error.message}`;
    console.error(error);
  }

  async function draw() {
    const current = metric();
    if (!current) {
      $("empty").hidden = false;
      $("empty").textContent = "No metrics were exported.";
      return;
    }
    const selected = projections(current);
    const mode = $("projection").value;
    $("single-control").hidden = mode !== "single";
    $("compare-control").hidden = mode !== "compare";
    $("chart-title").textContent = current.aggregate.label;
    $("metric-description").textContent = current.description;
    $("reference-key").textContent = `${current.reference_label} (${format(current.reference, current)})`;
    const traces = [];
    let valid = 0, partial = 0, missing = 0;
    selected.forEach(item => {
      const points = item.series.points;
      valid += points.filter(point => point.value !== null).length;
      partial += points.filter(point => point.value !== null && point.coverage.included < point.coverage.total).length;
      missing += points.filter(point => point.value === null).length;
      const hover = (point, index) => {
        const commit = snapshot.commits[index];
        return `<b>${escape(commit.hash.slice(0, 12))}</b><br>${escape(subject(commit))}<br>` +
          `${escape(item.label)}<br><b>${format(point.value, current)}</b><br>` +
          `Valid inputs: ${point.coverage.included}/${point.coverage.total}`;
      };
      const color = colors[item.style % colors.length];
      const common = {
        type: "scatter", name: item.label, legendgroup: item.id,
        hovertemplate: "%{text}<extra></extra>",
        customdata: points.map((_, index) => index),
        text: points.map(hover)
      };
      traces.push({...common, uid: item.id, mode: "lines+markers", connectgaps: false,
        x: points.map((_, index) => index), y: points.map(point => point.value),
        line: {color, width: 2, dash: dashes[Math.floor(item.style / colors.length) % dashes.length]},
        marker: {color, size: 7, symbol: symbols[item.style % symbols.length], line: {width: 1, color: "white"}},
        selectedpoints: pinned === null ? null : [pinned],
        selected: {marker: {color: "#d6368f", size: 10}}, unselected: {marker: {opacity: 1}}
      });
      // An independent, hidden axis gives missing data a clickable status rail;
      // these markers never stand in for zero on the metric axis.
      traces.push({...common, uid: `${item.id}-missing`, showlegend: false, yaxis: "y2", mode: "markers",
        x: points.map((point, index) => point.value === null ? index : null),
        y: points.map(point => point.value === null ? .025 : null),
        marker: {color: "#878b94", size: 9, symbol: "x"}
      });
    });
    $("empty").hidden = valid > 0;
    $("empty").textContent = selected.length === 0 ? "Choose coefficient sets to compare." :
      snapshot.commits.length === 0 ? "No commits in this experiment." :
      "No valid measurements. Select a gray marker or a commit below for exclusion reasons.";
    $("coverage-note").classList.toggle("partial", partial > 0 || missing > 0);
    $("coverage-note").textContent = `${valid} valid point(s) · ${partial} with partial input coverage · ${missing} missing. ` +
      (mode === "all" ? "Aggregates use the available valid inputs at each commit; changing coverage can affect comparisons. " : "Each line represents one frozen input. ") +
      "Gray × markers indicate missing measurements, not zero. Click any point for coverage and reasons.";
    const range = ready ? plot.layout.xaxis.range?.slice() : undefined;
    const xRange = range ?? [-.5, Math.max(.5, snapshot.commits.length - .5)];
    const ticks = ticksInRange(xRange);
    const layout = {
      uirevision: `experiment-${snapshot.experiment}`,
      margin: {l: 78, r: 26, t: 25, b: selected.length > 1 ? 80 + Math.ceil(selected.length / 2) * 22 : 75},
      font: {family: "system-ui, sans-serif", color: "#5a5c63", size: 11},
      paper_bgcolor: "white", plot_bgcolor: "white", dragmode,
      hovermode: "closest", hoverlabel: {bgcolor: "white", bordercolor: "#d7dce5", font: {size: 12}},
      showlegend: selected.length > 1,
      legend: {orientation: "h", x: 0, y: -.2, yanchor: "top", groupclick: "togglegroup", entrywidth: .48, entrywidthmode: "fraction"},
      xaxis: {title: {text: "Commit order", standoff: 14}, type: "linear", tickmode: "array", tickvals: ticks,
        ticktext: tickText(ticks),
        gridcolor: "#f3f4f7", zeroline: false, showline: true, linecolor: "#d7dce5",
        range: xRange, autorange: false,
        uirevision: "commits"},
      yaxis: {title: {text: current.aggregate.unit === "%" ? "Area savings (%)" : "Runtime ratio (Yosys / paper)", standoff: 14},
        gridcolor: "#edf0f5", zeroline: false, ticksuffix: current.aggregate.unit === "%" ? "%" : "×",
        autorange: true, autorangeoptions: {include: current.reference},
        uirevision: `${current.id}:${selected.map(item => item.id).join(",")}`},
      yaxis2: {overlaying: "y", range: [0, 1], visible: false, fixedrange: true},
      shapes: [{type: "line", xref: "paper", x0: 0, x1: 1, y0: current.reference, y1: current.reference,
        line: {color: "#d6368f", dash: "dash", width: 1}, layer: "below"}]
    };
    await Plotly.react(plot, traces, layout, {responsive: true, scrollZoom: true, displayModeBar: false,
      doubleClick: "reset", showTips: false, displaylogo: false});
    if (!ready) {
      plot.on("plotly_click", event => {
        const index = event.points[0]?.customdata;
        if (Number.isInteger(index)) pin(index);
      });
      plot.on("plotly_relayout", event => {
        if (!event["xaxis.range"] && !("xaxis.range[0]" in event)) return;
        const ticks = ticksInRange(plot.layout.xaxis.range);
        Plotly.relayout(plot, {"xaxis.tickvals": ticks, "xaxis.ticktext": tickText(ticks)});
      });
      ready = true;
    }
    details();
  }

  ["metric", "projection", "workload"].forEach(id => $(id).addEventListener("change", redraw));
  $("commit").addEventListener("change", () => {
    if ($("commit").value === "") return;
    pin(Number($("commit").value));
  });
  window.addEventListener("resize", () => {
    if (!ready) return;
    const ticks = ticksInRange(plot.layout.xaxis.range);
    Plotly.relayout(plot, {"xaxis.tickvals": ticks, "xaxis.ticktext": tickText(ticks)});
  });
  ["pan", "zoom"].forEach(id => $(id).addEventListener("click", () => {
    dragmode = id === "pan" ? "pan" : "zoom";
    $("pan").setAttribute("aria-pressed", String(id === "pan"));
    $("zoom").setAttribute("aria-pressed", String(id === "zoom"));
    if (ready) Plotly.relayout(plot, {dragmode});
  }));
  $("reset").addEventListener("click", () => {
    if (ready) Plotly.relayout(plot, {"xaxis.range": [-.5, Math.max(.5, snapshot.commits.length - .5)], "yaxis.autorange": true});
  });
  $("export").addEventListener("click", () => {
    if (ready) Plotly.downloadImage(plot, {format: "svg", filename: `mcm-experiment-${snapshot.experiment}-${metric().id}`, width: 1400, height: 700}).catch(showError);
  });
  await redraw();
})().catch(error => {
  document.getElementById("error").hidden = false;
  document.getElementById("error").textContent = `Unable to load snapshot: ${error.message}`;
});
