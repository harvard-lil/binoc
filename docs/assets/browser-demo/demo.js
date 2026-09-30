import {
  WASI,
  File,
  OpenFile,
  ConsoleStdout,
  PreopenDirectory,
  Directory,
} from "./vendor/browser_wasi_shim/index.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function bytesFromBase64(value) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

const demos = {
  "csv-keyed-row-diff": {
    label: "CSV keyed row diff",
    command:
      "binoc diff snapshots/csv-keyed-row-diff/snapshot-a snapshots/csv-keyed-row-diff/snapshot-b --config snapshots/csv-keyed-row-diff/config.yaml",
    files: {
      "snapshot-a/data.csv":
        "id,name,price,status\np1,Alpha,10,active\np2,Beta,20,active\np3,Gamma,30,retired\n",
      "snapshot-b/data.csv":
        "id,name,price,status\np2,Beta,25,active\np4,Delta,40,active\np1,Alpha,10,active\n",
      "config.yaml":
        "dataset:\n  paths:\n    - match: data.csv\n      row_identity:\n        columns: [id]\n",
    },
  },
  "observations-split-by-year": {
    label: "CSV split into yearly files",
    command:
      "binoc diff snapshots/observations-split-by-year/snapshot-a snapshots/observations-split-by-year/snapshot-b",
    files: {
      "snapshot-a/observations.csv":
        "year,region,count\n2024,north,10\n2024,south,12\n2025,north,15\n2025,south,9\n",
      "snapshot-b/observations_2024.csv":
        "year,region,count\n2024,north,10\n2024,south,12\n",
      "snapshot-b/observations_2025.csv":
        "year,region,count\n2025,north,15\n2025,south,9\n",
    },
  },
  "sqlite-row-addition": {
    label: "SQLite row addition",
    command:
      "binoc diff snapshots/sqlite-row-addition/snapshot-a snapshots/sqlite-row-addition/snapshot-b",
    files: {
      "snapshot-a/data.sqlite": bytesFromBase64(
        "U1FMaXRlIGZvcm1hdCAzAAIAAQEMQCAgAAAAAwAAAAIAAAAAAAAAAAAAAAIAAAAEAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADAC5uug0AAAABAaYAAaYAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABMAQYXFxcBeXRhYmxldXNlcnN1c2VycwJDUkVBVEUgVEFCTEUgdXNlcnMgKGlkIElOVEVHRVIgUFJJTUFSWSBLRVksIG5hbWUgVEVYVCkAAAAAAAAAAAAAAAANAAAAAQHsAAHsAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGAQMAE0FkYQAAAAAAAAAAAAAAAA==",
      ),
      "snapshot-b/data.sqlite": bytesFromBase64(
        "U1FMaXRlIGZvcm1hdCAzAAIAAQEMQCAgAAAABAAAAAIAAAAAAAAAAAAAAAIAAAAEAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAC5uug0AAAABAaYAAaYAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABMAQYXFxcBeXRhYmxldXNlcnN1c2VycwJDUkVBVEUgVEFCTEUgdXNlcnMgKGlkIElOVEVHRVIgUFJJTUFSWSBLRVksIG5hbWUgVEVYVCkAAAAAAAAAAAAAAAANAAAAAgHiAAHsAeIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgCAwAXR3JhY2UGAQMAE0FkYQAAAAAAAAAAAAAAAA==",
      ),
    },
  },
};

const state = {
  compiled: null,
  activeDemo: "csv-keyed-row-diff",
  selectedPath: "",
  history: [],
  historyIndex: 0,
  running: false,
};

function file(contents) {
  const bytes = typeof contents === "string" ? encoder.encode(contents) : contents;
  return new File(bytes, { readonly: true });
}

function directory(entries = []) {
  return new Directory(entries);
}

function insertPath(root, path, contents) {
  const parts = path.split("/");
  let dir = root;
  for (const part of parts.slice(0, -1)) {
    let child = dir.contents.get(part);
    if (!child) {
      child = directory();
      child.parent = dir;
      dir.contents.set(part, child);
    }
    dir = child;
  }
  dir.contents.set(parts[parts.length - 1], file(contents));
}

function buildSnapshotsDirectory() {
  const snapshots = directory();
  for (const [demoName, demo] of Object.entries(demos)) {
    const demoDir = directory();
    demoDir.parent = snapshots;
    snapshots.contents.set(demoName, demoDir);
    for (const [relativePath, text] of Object.entries(demo.files)) {
      insertPath(demoDir, relativePath, text);
    }
  }
  return snapshots;
}

function buildRootDirectory() {
  return directory([["snapshots", buildSnapshotsDirectory()]]);
}

function flattenDemoFiles(demo) {
  return Object.keys(demo.files).sort();
}

function renderTree() {
  const tree = document.querySelector("[data-browser-demo-tree]");
  const demo = demos[state.activeDemo];
  tree.textContent = "";
  for (const path of flattenDemoFiles(demo)) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = path;
    button.style.paddingLeft = `${0.35 + path.split("/").length * 0.45}rem`;
    button.setAttribute("aria-current", String(path === state.selectedPath));
    button.addEventListener("click", () => {
      state.selectedPath = path;
      renderTree();
      renderContent();
    });
    tree.appendChild(button);
  }
}

function renderContent() {
  const path = state.selectedPath || flattenDemoFiles(demos[state.activeDemo])[0];
  const contents = demos[state.activeDemo].files[path];
  state.selectedPath = path;
  document.querySelector("[data-browser-demo-file]").textContent = path;
  document.querySelector("[data-browser-demo-content]").textContent =
    typeof contents === "string"
      ? contents
      : `Binary SQLite database (${contents.byteLength} bytes)`;
}

function terminalWrite(text) {
  const output = document.querySelector("[data-browser-demo-output]");
  output.textContent += text;
  output.scrollTop = output.scrollHeight;
}

function setStatus(text) {
  document.querySelector("[data-browser-demo-status]").textContent = text;
}

function setRunning(running) {
  state.running = running;
  document.querySelector("[data-browser-demo-input]").disabled = running;
  document.querySelector("[data-browser-demo-run]").disabled = running;
  setStatus(running ? "running" : "ready");
}

async function wasmModule() {
  if (!state.compiled) {
    setStatus("loading wasm");
    const response = await fetch("../../../assets/browser-demo/wasm/binoc-cli.wasm");
    state.compiled = await WebAssembly.compile(await response.arrayBuffer());
  }
  return state.compiled;
}

function shellSplit(command) {
  const parts = [];
  let current = "";
  let quote = "";
  for (const char of command.trim()) {
    if (quote) {
      if (char === quote) quote = "";
      else current += char;
    } else if (char === "'" || char === '"') {
      quote = char;
    } else if (/\s/.test(char)) {
      if (current) {
        parts.push(current);
        current = "";
      }
    } else {
      current += char;
    }
  }
  if (current) parts.push(current);
  return parts;
}

async function runBinoc(args) {
  let stdout = "";
  let stderr = "";
  const fds = [
    new OpenFile(new File([])),
    ConsoleStdout.lineBuffered((line) => {
      stdout += `${line}\n`;
    }),
    ConsoleStdout.lineBuffered((line) => {
      stderr += `${line}\n`;
    }),
    new PreopenDirectory(".", Array.from(buildRootDirectory().contents.entries())),
  ];
  const wasi = new WASI(args, [], fds, { debug: false });
  const instance = await WebAssembly.instantiate(await wasmModule(), {
    wasi_snapshot_preview1: wasi.wasiImport,
  });
  const code = wasi.start(instance);
  return { code, stdout, stderr };
}

async function runCommand(rawCommand) {
  const command = rawCommand.trim();
  if (!command) return;
  terminalWrite(`$ ${command}\n`);
  if (command === "clear") {
    document.querySelector("[data-browser-demo-output]").textContent = "";
    return;
  }
  const args = shellSplit(command);
  if (args[0] !== "binoc") {
    terminalWrite("Only binoc commands are available in this browser demo.\n\n");
    return;
  }
  setRunning(true);
  try {
    const result = await runBinoc(args);
    if (result.stdout) terminalWrite(result.stdout);
    if (result.stderr) terminalWrite(result.stderr);
    if (result.code !== 0) terminalWrite(`exit ${result.code}\n`);
    terminalWrite("\n");
  } catch (error) {
    terminalWrite(`${error?.message || error}\n\n`);
  } finally {
    setRunning(false);
  }
}

function updateDemo(name) {
  state.activeDemo = name;
  state.selectedPath = flattenDemoFiles(demos[name])[0];
  document.querySelector("[data-browser-demo-input]").value = demos[name].command;
  renderTree();
  renderContent();
}

function initBrowserDemo() {
  const root = document.querySelector("[data-browser-demo]");
  if (!root) return;

  const select = document.querySelector("[data-browser-demo-select]");
  for (const [name, demo] of Object.entries(demos)) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = demo.label;
    select.appendChild(option);
  }
  select.addEventListener("change", () => updateDemo(select.value));

  const form = document.querySelector("[data-browser-demo-form]");
  const input = document.querySelector("[data-browser-demo-input]");
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (state.running) return;
    const command = input.value;
    state.history.push(command);
    state.historyIndex = state.history.length;
    runCommand(command);
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowUp" && state.history.length) {
      event.preventDefault();
      state.historyIndex = Math.max(0, state.historyIndex - 1);
      input.value = state.history[state.historyIndex];
    } else if (event.key === "ArrowDown" && state.history.length) {
      event.preventDefault();
      state.historyIndex = Math.min(state.history.length, state.historyIndex + 1);
      input.value = state.history[state.historyIndex] || demos[state.activeDemo].command;
    }
  });

  updateDemo(state.activeDemo);
  terminalWrite("binoc browser demo\n");
}

if (window.document$) {
  document$.subscribe(initBrowserDemo);
} else {
  document.addEventListener("DOMContentLoaded", initBrowserDemo);
}
