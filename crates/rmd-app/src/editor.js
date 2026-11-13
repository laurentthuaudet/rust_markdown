// Editor helpers that need the DOM: line geometry and editor/preview scroll sync.
//
// Installed once; works with whatever `.editor` / `.preview` elements are
// currently on the page, since Dioxus recreates them when the view mode changes.
if (!window.rmdEditor) {
  const editor = () => document.querySelector("textarea.editor");
  const preview = () => document.querySelector(".preview");

  // ---- Line geometry -------------------------------------------------------
  // A textarea wraps long lines, so line N is not at N * lineHeight. A hidden
  // mirror with the same width and font gives the real top of every line.
  let geometry = null; // { tops: number[], heights: number[], padTop: number }

  function measure() {
    const ta = editor();
    if (!ta) return (geometry = null);
    let mirror = document.getElementById("rmd-mirror");
    if (!mirror) {
      mirror = document.createElement("div");
      mirror.id = "rmd-mirror";
      mirror.setAttribute("data-rmd-generated", "");
      document.body.appendChild(mirror);
    }
    const cs = getComputedStyle(ta);
    const padLeft = parseFloat(cs.paddingLeft);
    const padRight = parseFloat(cs.paddingRight);
    Object.assign(mirror.style, {
      position: "absolute",
      visibility: "hidden",
      top: "0",
      left: "-10000px",
      whiteSpace: "pre-wrap",
      overflowWrap: "break-word",
      width: `${ta.clientWidth - padLeft - padRight}px`,
      font: cs.font,
      lineHeight: cs.lineHeight,
      letterSpacing: cs.letterSpacing,
      tabSize: cs.tabSize,
    });
    const lines = ta.value.split("\n");
    const fragment = document.createDocumentFragment();
    for (const line of lines) {
      const div = document.createElement("div");
      div.textContent = line || "​";
      fragment.appendChild(div);
    }
    mirror.replaceChildren(fragment);
    const tops = [];
    const heights = [];
    for (const child of mirror.children) {
      tops.push(child.offsetTop);
      heights.push(child.offsetHeight);
    }
    geometry = { tops, heights, padTop: parseFloat(cs.paddingTop) };
    window.dispatchEvent(new CustomEvent("rmd-geometry"));
    return geometry;
  }

  let measureQueued = false;
  function queueMeasure() {
    if (measureQueued) return;
    measureQueued = true;
    requestAnimationFrame(() => {
      measureQueued = false;
      measure();
    });
  }

  // Fractional (0-based) line at vertical offset `y` of the editor content.
  function lineAt(y) {
    const { tops, heights } = geometry;
    let lo = 0;
    let hi = tops.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (tops[mid] <= y) lo = mid;
      else hi = mid - 1;
    }
    const fraction = heights[lo] ? (y - tops[lo]) / heights[lo] : 0;
    return lo + Math.min(Math.max(fraction, 0), 1);
  }

  function offsetOfLine(line) {
    const { tops, heights } = geometry;
    const index = Math.min(Math.floor(line), tops.length - 1);
    return tops[index] + (line - index) * heights[index];
  }

  // ---- Preview anchors -----------------------------------------------------
  // [{ line (0-based), top }] for every block carrying data-sourcepos.
  let anchors = null;

  function previewAnchors() {
    if (anchors) return anchors;
    const pv = preview();
    if (!pv) return [];
    const base = pv.getBoundingClientRect().top - pv.scrollTop;
    const list = [];
    for (const el of pv.querySelectorAll("[data-sourcepos]")) {
      const line = parseInt(el.getAttribute("data-sourcepos"), 10) - 1;
      const top = el.getBoundingClientRect().top - base;
      // Keep the map monotonic: nested blocks may start on the same line.
      const last = list[list.length - 1];
      if (!last || (line > last.line && top >= last.top)) list.push({ line, top });
    }
    return (anchors = list);
  }

  function interpolate(list, value, from, to) {
    if (!list.length) return null;
    let i = 0;
    while (i + 1 < list.length && list[i + 1][from] <= value) i++;
    const a = list[i];
    const b = list[i + 1];
    if (!b || b[from] === a[from]) return a[to] + (value - a[from]);
    return a[to] + ((b[to] - a[to]) * (value - a[from])) / (b[from] - a[from]);
  }

  // ---- Scroll sync ---------------------------------------------------------
  // Each side ignores the scroll event it caused on the other one.
  let ignoreEditor = false;
  let ignorePreview = false;

  function atBottom(el) {
    return el.scrollTop + el.clientHeight >= el.scrollHeight - 2;
  }

  function syncPreview() {
    const ta = editor();
    const pv = preview();
    if (!ta || !pv || !(geometry || measure())) return;
    let target;
    if (atBottom(ta)) target = pv.scrollHeight;
    else if (ta.scrollTop === 0) target = 0;
    else {
      const line = lineAt(ta.scrollTop - geometry.padTop);
      target = interpolate(previewAnchors(), line, "line", "top");
      if (target === null) return;
    }
    ignorePreview = true;
    pv.scrollTop = target;
  }

  function syncEditor() {
    const ta = editor();
    const pv = preview();
    if (!ta || !pv || !(geometry || measure())) return;
    let target;
    if (atBottom(pv)) target = ta.scrollHeight;
    else if (pv.scrollTop === 0) target = 0;
    else {
      const line = interpolate(previewAnchors(), pv.scrollTop, "top", "line");
      if (line === null) return;
      target = offsetOfLine(Math.max(line, 0)) + geometry.padTop;
    }
    ignoreEditor = true;
    ta.scrollTop = target;
  }

  // `scroll` does not bubble: listen in the capture phase.
  document.addEventListener(
    "scroll",
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (target.matches("textarea.editor")) {
        if (ignoreEditor) ignoreEditor = false;
        else requestAnimationFrame(syncPreview);
      } else if (target.matches(".preview")) {
        if (ignorePreview) ignorePreview = false;
        else requestAnimationFrame(syncEditor);
      }
    },
    true,
  );

  document.addEventListener("input", (event) => {
    if (event.target.matches && event.target.matches("textarea.editor")) queueMeasure();
  });
  window.addEventListener("resize", () => {
    anchors = null;
    queueMeasure();
  });

  // The preview is re-rendered by Dioxus on every edit: drop the anchor map.
  // New editor/preview elements (view mode change, file opened) need a remeasure.
  // Elements this script generates itself are marked `data-rmd-generated` and
  // ignored, otherwise updating them would trigger an endless remeasure.
  const root = document.getElementById("main") || document.body;
  new MutationObserver((records) => {
    const relevant = records.some((record) => {
      const node = record.target.nodeType === 1 ? record.target : record.target.parentElement;
      return node && !node.closest("[data-rmd-generated]");
    });
    if (!relevant) return;
    anchors = null;
    queueMeasure();
  }).observe(root, { childList: true, subtree: true, characterData: true });

  window.rmdEditor = { measure, lineAt, offsetOfLine, geometry: () => geometry };
}
