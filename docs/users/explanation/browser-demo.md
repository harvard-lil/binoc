---
audience: anyone evaluating Binoc in the browser
---

# Browser demo

This prototype runs the Rust CLI as a WASI module inside the page. The snapshots
are mounted into an in-memory filesystem; the terminal invokes `binoc` against
that virtual filesystem and renders the real command output.

<link rel="stylesheet" href="../../../assets/browser-demo/demo.css">

<div class="browser-demo" data-browser-demo>
  <div class="browser-demo__bar">
    <select class="browser-demo__select" data-browser-demo-select aria-label="Demo dataset"></select>
    <div class="browser-demo__status" data-browser-demo-status>ready</div>
  </div>
  <div class="browser-demo__grid">
    <section class="browser-demo__pane" aria-label="File tree">
      <div class="browser-demo__pane-head">Files</div>
      <div class="browser-demo__tree" data-browser-demo-tree></div>
    </section>
    <section class="browser-demo__pane" aria-label="File content">
      <div class="browser-demo__pane-head"><span data-browser-demo-file></span></div>
      <pre class="browser-demo__content"><code data-browser-demo-content></code></pre>
    </section>
    <section class="browser-demo__pane" aria-label="Binoc terminal">
      <div class="browser-demo__pane-head">Terminal</div>
      <div class="browser-demo__terminal">
        <pre class="browser-demo__terminal-output" data-browser-demo-output></pre>
        <form class="browser-demo__terminal-form" data-browser-demo-form>
          <span class="browser-demo__prompt">$</span>
          <input class="browser-demo__input" data-browser-demo-input aria-label="Command" spellcheck="false">
          <button class="browser-demo__run" data-browser-demo-run type="submit">Run</button>
        </form>
      </div>
    </section>
  </div>
</div>

<script type="module" src="../../../assets/browser-demo/demo.js"></script>

## Current scope

The current page proves the stdlib and first-party SQLite paths: directory
walking, CSV parsing, SQLite parsing, artifact storage, row-key config,
correspondence, and Markdown rendering all run inside the browser.
