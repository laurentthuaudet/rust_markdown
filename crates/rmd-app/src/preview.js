// Internal links of the preview (footnotes, table of contents: href="#…").
//
// Dioxus intercepts every link click and opens it in the system browser.
// This capture-phase listener runs first: for in-document anchors it stops
// the event and scrolls the preview to the target instead. External links
// are left alone and still open in the browser.
if (!window.__rmdAnchors) {
  window.__rmdAnchors = true;
  window.addEventListener(
    "click",
    (event) => {
      const link = event.target.closest && event.target.closest(".preview a[href^='#']");
      if (!link) return;
      event.preventDefault();
      event.stopPropagation();
      const id = decodeURIComponent(link.getAttribute("href").slice(1));
      if (!id) return;
      const preview = link.closest(".preview");
      const target =
        preview.querySelector(`[id="${CSS.escape(id)}"]`) ||
        preview.querySelector(`[name="${CSS.escape(id)}"]`);
      if (!target) return;
      target.scrollIntoView({ behavior: "smooth", block: "center" });
      target.classList.remove("anchor-target");
      void target.offsetWidth; // restart the highlight animation
      target.classList.add("anchor-target");
    },
    true,
  );
}
