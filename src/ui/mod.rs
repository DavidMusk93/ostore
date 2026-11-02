//! Modern Single-Page Application (SPA) Web UI for ostore.
//!
//! Conforming to Apple-design language with an authentic light theme:
//! - Background #F5F5F7, Cards rgba(255,255,255,0.88), Ink #1D1D1F, Accent #0071E3.
//! - Naming topology: Main & Follower nodes.
//! - Per-file replica topology inspector with accurate honest states and metrics.
//! - Zero charset ambiguity: explicit UTF-8 TextDecoder.
//! - Fast tiered probe with detailed latency diagnostics.
//! - SSE-only incremental reactive updates (zero polling).
//! - Virtual/chunked lazy rendering for 60fps listing performance.

use axum::response::Html;

pub async fn handler() -> Html<&'static str> {
    Html(UI_HTML)
}

const UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>ostore — Distributed Object Storage</title>
  <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='%230071E3'><path d='M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5'/></svg>">
  <style>
    :root {
      --bg: #F5F5F7;
      --card-bg: rgba(255, 255, 255, 0.88);
      --card-border: rgba(0, 0, 0, 0.08);
      --card-hover: rgba(0, 0, 0, 0.025);
      --ink: #1D1D1F;
      --ink-secondary: #86868B;
      --ink-tertiary: #AEAEB2;
      --accent: #0071E3;
      --accent-hover: #0077ED;
      --accent-soft: rgba(0, 113, 227, 0.1);
      --success: #34C759;
      --success-soft: rgba(52, 199, 89, 0.12);
      --warning: #FF9500;
      --warning-soft: rgba(255, 149, 0, 0.12);
      --danger: #FF3B30;
      --danger-soft: rgba(255, 59, 48, 0.12);
      --radius-sm: 8px;
      --radius-md: 14px;
      --radius-lg: 20px;
      --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.04), 0 4px 12px rgba(0, 0, 0, 0.02);
      --shadow-md: 0 4px 16px rgba(0, 0, 0, 0.06), 0 1px 2px rgba(0, 0, 0, 0.04);
      --font-sans: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif;
      --font-mono: ui-monospace, "SF Mono", Menlo, Monaco, Consolas, monospace;
    }

    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: var(--font-sans);
      background-color: var(--bg);
      color: var(--ink);
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      -webkit-font-smoothing: antialiased;
    }

    .container {
      max-width: 1260px;
      margin: 0 auto;
      padding: 32px 24px;
      width: 100%;
      flex: 1;
    }

    /* Eyebrow numbering */
    .eyebrow {
      font-family: var(--font-mono);
      font-size: 0.72rem;
      font-weight: 600;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: var(--ink-secondary);
      margin-bottom: 8px;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .eyebrow::after {
      content: "";
      flex: 1;
      height: 1px;
      background: var(--card-border);
    }

    /* Header */
    header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-bottom: 24px;
      margin-bottom: 28px;
      border-bottom: 1px solid var(--card-border);
      flex-wrap: wrap;
      gap: 16px;
    }

    .brand {
      display: flex;
      align-items: center;
      gap: 12px;
      text-decoration: none;
      color: var(--ink);
    }

    .brand-logo {
      width: 36px;
      height: 36px;
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: 10px;
      display: flex;
      align-items: center;
      justify-content: center;
      box-shadow: var(--shadow-sm);
    }

    .brand-logo svg { width: 20px; height: 20px; fill: var(--accent); }
    .brand-title { font-size: 1.25rem; font-weight: 600; letter-spacing: -0.02em; }
    .brand-subtitle { font-size: 0.76rem; color: var(--ink-secondary); }

    .header-actions {
      display: flex;
      align-items: center;
      gap: 10px;
    }

    .status-pill {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 6px 12px;
      border-radius: 980px;
      font-size: 0.75rem;
      font-family: var(--font-mono);
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      color: var(--ink-secondary);
      box-shadow: var(--shadow-sm);
    }

    .status-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--success);
      box-shadow: 0 0 6px rgba(52, 199, 89, 0.4);
    }

    .status-dot.disconnected {
      background: var(--danger);
      box-shadow: 0 0 6px rgba(255, 59, 48, 0.4);
    }

    .btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 7px 14px;
      border-radius: var(--radius-sm);
      font-size: 0.82rem;
      font-weight: 500;
      cursor: pointer;
      border: 1px solid var(--card-border);
      background: #FFFFFF;
      color: var(--ink);
      transition: all 0.15s ease;
      box-shadow: var(--shadow-sm);
    }

    .btn:hover {
      background: #F9F9FB;
      border-color: rgba(0, 0, 0, 0.15);
    }

    .btn-primary {
      background: var(--accent);
      border-color: var(--accent);
      color: #FFFFFF;
      box-shadow: 0 2px 8px rgba(0, 113, 227, 0.25);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
    }

    /* Panels */
    .panel {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--radius-md);
      backdrop-filter: blur(20px);
      box-shadow: var(--shadow-sm);
      margin-bottom: 28px;
      overflow: hidden;
    }

    .panel-header {
      padding: 16px 20px;
      border-bottom: 1px solid var(--card-border);
      display: flex;
      justify-content: space-between;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
    }

    .panel-title {
      font-size: 0.95rem;
      font-weight: 600;
      letter-spacing: -0.01em;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    /* Topology Layout */
    .topology-wrap {
      padding: 20px;
      display: grid;
      grid-template-columns: 1fr 360px;
      gap: 20px;
    }

    @media (max-width: 960px) {
      .topology-wrap { grid-template-columns: 1fr; }
    }

    .topo-canvas-container {
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      min-height: 220px;
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .topo-cards-grid {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }

    .topo-card {
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      padding: 12px 14px;
      box-shadow: var(--shadow-sm);
    }

    .topo-card-head {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 6px;
    }

    .topo-card-title {
      font-size: 0.84rem;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 6px;
    }

    .topo-card-badge {
      font-size: 0.68rem;
      font-family: var(--font-mono);
      padding: 2px 6px;
      border-radius: 4px;
      background: rgba(0, 0, 0, 0.04);
      color: var(--ink-secondary);
    }

    .topo-card-meta {
      font-size: 0.75rem;
      color: var(--ink-secondary);
      display: flex;
      justify-content: space-between;
      margin-bottom: 6px;
      font-family: var(--font-mono);
    }

    .meter-bar {
      height: 4px;
      background: rgba(0, 0, 0, 0.05);
      border-radius: 2px;
      overflow: hidden;
    }

    .meter-fill {
      height: 100%;
      background: var(--accent);
      border-radius: 2px;
    }

    /* Distribution Spectrum */
    .spectrum-panel {
      padding: 14px 20px;
      border-top: 1px solid var(--card-border);
      background: rgba(255, 255, 255, 0.5);
    }

    .spectrum-bar {
      height: 6px;
      border-radius: 980px;
      background: rgba(0, 0, 0, 0.06);
      display: flex;
      overflow: hidden;
      margin-bottom: 10px;
    }

    .spectrum-segment { height: 100%; transition: width 0.3s; }

    .spectrum-legend {
      display: flex;
      flex-wrap: wrap;
      gap: 16px;
      font-size: 0.74rem;
      color: var(--ink-secondary);
    }

    .spectrum-legend-item {
      display: flex;
      align-items: center;
      gap: 5px;
    }

    .legend-dot { width: 7px; height: 7px; border-radius: 2px; }

    /* Explorer View */
    .explorer-bar {
      padding: 12px 20px;
      border-bottom: 1px solid var(--card-border);
      display: flex;
      justify-content: space-between;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
    }

    .breadcrumbs {
      display: flex;
      align-items: center;
      gap: 4px;
      font-size: 0.85rem;
      font-family: var(--font-mono);
      flex-wrap: wrap;
    }

    .crumb-link {
      color: var(--ink-secondary);
      cursor: pointer;
      text-decoration: none;
      padding: 3px 6px;
      border-radius: 4px;
      transition: all 0.12s;
    }

    .crumb-link:hover {
      color: var(--ink);
      background: rgba(0, 0, 0, 0.04);
    }

    .crumb-sep {
      color: var(--ink-tertiary);
      font-size: 0.75rem;
    }

    .search-input {
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      padding: 6px 12px;
      color: var(--ink);
      font-size: 0.82rem;
      width: 180px;
      outline: none;
      transition: all 0.2s;
    }

    .search-input:focus {
      width: 240px;
      border-color: var(--accent);
      box-shadow: 0 0 0 2px var(--accent-soft);
    }

    /* Table */
    .table-viewport {
      max-height: 600px;
      overflow-y: auto;
      position: relative;
    }

    .file-table {
      width: 100%;
      border-collapse: collapse;
      text-align: left;
      font-size: 0.84rem;
    }

    .file-table th {
      position: sticky;
      top: 0;
      background: #FAFAFC;
      padding: 10px 20px;
      color: var(--ink-secondary);
      font-weight: 500;
      font-size: 0.72rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      border-bottom: 1px solid var(--card-border);
      z-index: 10;
    }

    .file-row {
      border-bottom: 1px solid rgba(0, 0, 0, 0.04);
      transition: background 0.12s;
    }

    .file-row:hover {
      background: var(--card-hover);
    }

    .file-row td {
      padding: 10px 20px;
      vertical-align: middle;
    }

    .name-cell {
      display: flex;
      align-items: center;
      gap: 10px;
      cursor: pointer;
      font-weight: 500;
    }

    .name-cell svg {
      width: 16px;
      height: 16px;
      flex-shrink: 0;
    }

    .meta-mono {
      font-family: var(--font-mono);
      font-size: 0.76rem;
      color: var(--ink-secondary);
    }

    /* Per-file Replica Status Pill: HONEST STATES */
    .replica-pill {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      padding: 3px 9px;
      border-radius: 980px;
      font-family: var(--font-mono);
      font-size: 0.72rem;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.15s;
      border: 1px solid transparent;
    }

    .replica-pill.probe-needed {
      background: rgba(0, 0, 0, 0.04);
      color: var(--ink-secondary);
      border-color: rgba(0, 0, 0, 0.06);
    }

    .replica-pill.synced {
      background: var(--success-soft);
      color: #248A3D;
    }

    .replica-pill.main-only {
      background: var(--warning-soft);
      color: #B25E00;
    }

    .replica-pill.degraded {
      background: var(--warning-soft);
      color: #B25E00;
    }

    .replica-pill:hover {
      filter: brightness(0.96);
    }

    .sha-tag {
      font-family: var(--font-mono);
      font-size: 0.72rem;
      padding: 2px 6px;
      background: rgba(0, 0, 0, 0.04);
      border-radius: 4px;
      color: var(--ink-secondary);
      cursor: pointer;
    }

    .sha-tag:hover {
      color: var(--ink);
      background: rgba(0, 0, 0, 0.08);
    }

    /* Row actions: Controls on intent (hover only) */
    .row-actions {
      display: flex;
      align-items: center;
      gap: 6px;
      justify-content: flex-end;
      opacity: 0;
      transition: opacity 0.15s ease;
    }

    .file-row:hover .row-actions {
      opacity: 1;
    }

    .icon-btn {
      background: transparent;
      border: none;
      padding: 5px;
      border-radius: 4px;
      cursor: pointer;
      color: var(--ink-secondary);
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.12s;
    }

    .icon-btn:hover {
      color: var(--ink);
      background: rgba(0, 0, 0, 0.06);
    }

    .icon-btn.btn-delete:hover {
      color: var(--danger);
      background: var(--danger-soft);
    }

    /* Modals & Backdrops */
    .backdrop {
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.4);
      backdrop-filter: blur(10px);
      z-index: 500;
      display: none;
      align-items: center;
      justify-content: center;
      padding: 20px;
    }

    .dialog-box {
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-md);
      padding: 24px;
      width: 100%;
      max-width: 500px;
      box-shadow: var(--shadow-md);
    }

    .dialog-box h3 {
      font-size: 1.05rem;
      font-weight: 600;
      margin-bottom: 12px;
    }

    /* Per-file Topology Inspector */
    .file-topo-box {
      max-width: 620px;
    }

    .file-topo-flow {
      display: flex;
      flex-direction: column;
      gap: 10px;
      margin: 16px 0;
    }

    .file-node-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 10px 14px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--card-border);
      background: #F9F9FB;
    }

    .metrics-tag {
      font-family: var(--font-mono);
      font-size: 0.72rem;
      color: var(--ink-secondary);
      background: rgba(0, 0, 0, 0.04);
      padding: 4px 8px;
      border-radius: 4px;
      margin-top: 8px;
      display: flex;
      justify-content: space-between;
    }

    /* Toast */
    .toast {
      position: fixed;
      bottom: 24px;
      right: 24px;
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      padding: 10px 18px;
      border-radius: var(--radius-sm);
      font-size: 0.82rem;
      box-shadow: var(--shadow-md);
      display: none;
      z-index: 1000;
      align-items: center;
      gap: 8px;
    }
  </style>
</head>
<body>
  <div class="container">
    <!-- Header -->
    <header>
      <a href="#" class="brand" onclick="navigate(''); return false;">
        <div class="brand-logo">
          <svg viewBox="0 0 24 24"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>
        </div>
        <div>
          <div class="brand-title">ostore</div>
          <div class="brand-subtitle">Distributed Object Storage Control Plane</div>
        </div>
      </a>

      <div class="header-actions">
        <div class="status-pill">
          <span class="status-dot" id="sseDot"></span>
          <span id="sseStatus">SSE Stream Ready</span>
        </div>
        <button class="btn" onclick="openTokenModal()">
          <span id="tokenStatusText">Auth Token</span>
        </button>
        <button class="btn btn-primary" onclick="triggerSync()">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
          Sync Followers
        </button>
      </div>
    </header>

    <!-- 01 . CLUSTER TOPOLOGY GRAPH -->
    <div class="eyebrow">01 . Main & Follower Fleet Topology</div>
    <section class="panel">
      <div class="panel-header">
        <span class="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/></svg>
          Topology & Replication Network
        </span>
        <span style="font-family: var(--font-mono); font-size: 0.76rem; color: var(--ink-secondary);" id="clusterCapacityText">
          Loading topology...
        </span>
      </div>

      <div class="topology-wrap">
        <!-- SVG Canvas for Main -> Followers -->
        <div class="topo-canvas-container">
          <svg id="topoCanvas" width="100%" height="220" viewBox="0 0 540 220" style="overflow: visible;">
            <!-- Dynamically generated SVG -->
          </svg>
        </div>

        <!-- Node Status Cards -->
        <div class="topo-cards-grid" id="topoCardsList"></div>
      </div>

      <!-- Namespace Distribution Spectrum -->
      <div class="spectrum-panel">
        <div style="display: flex; justify-content: space-between; font-size: 0.74rem; color: var(--ink-secondary); margin-bottom: 6px; font-family: var(--font-mono);">
          <span>DATA DISTRIBUTION SPECTRUM</span>
          <span id="spectrumSummaryText">-- objects</span>
        </div>
        <div class="spectrum-bar" id="spectrumBar"></div>
        <div class="spectrum-legend" id="spectrumLegend"></div>
      </div>
    </section>

    <!-- 02 . OBJECT STORAGE EXPLORER -->
    <div class="eyebrow">02 . Object Storage Explorer</div>
    <section class="panel">
      <div class="explorer-bar">
        <div class="breadcrumbs" id="breadcrumbs">
          <span class="crumb-link" onclick="navigate('')">root</span>
        </div>
        <div style="display: flex; gap: 8px; align-items: center;">
          <input type="text" class="search-input" id="searchBox" placeholder="Filter objects..." oninput="onFilter()">
          <button class="btn" onclick="openMkdir()">New Folder</button>
          <button class="btn btn-primary" onclick="triggerFileInput()">Upload Object</button>
          <input type="file" id="fileInput" multiple style="display: none;" onchange="handleFileSelect(event)">
        </div>
      </div>

      <div class="table-viewport" id="tableViewport">
        <table class="file-table">
          <thead>
            <tr>
              <th style="width: 44%;">Name</th>
              <th style="width: 12%;">Size</th>
              <th style="width: 16%;">Modified</th>
              <th style="width: 18%;">Replica Status</th>
              <th style="width: 10%; text-align: right;">Actions</th>
            </tr>
          </thead>
          <tbody id="fileTableBody"></tbody>
        </table>
        <div id="scrollAnchor" style="height: 20px;"></div>
      </div>
    </section>
  </div>

  <!-- Token Modal -->
  <div class="backdrop" id="tokenModal">
    <div class="dialog-box">
      <h3>Access Token</h3>
      <p style="font-size: 0.82rem; color: var(--ink-secondary); margin-bottom: 12px;">Stored in browser localStorage for direct API access:</p>
      <input type="password" id="tokenInput" class="search-input" style="width: 100%;" placeholder="OSTORE_TOKEN">
      <div style="display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px;">
        <button class="btn" onclick="closeTokenModal()">Cancel</button>
        <button class="btn btn-primary" onclick="saveToken()">Save</button>
      </div>
    </div>
  </div>

  <!-- Per-file Topology Modal -->
  <div class="backdrop" id="fileTopoModal">
    <div class="dialog-box file-topo-box">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <h3 id="fileTopoTitle">File Replica Topology</h3>
        <button class="icon-btn" onclick="closeFileTopoModal()">✕</button>
      </div>
      <p style="font-size: 0.78rem; color: var(--ink-secondary); font-family: var(--font-mono); margin-bottom: 12px;" id="fileTopoPath"></p>
      
      <div class="file-topo-flow" id="fileTopoFlow"></div>

      <div class="metrics-tag" id="fileTopoMetrics">
        <span>Probe Latency: --</span>
        <span id="fileTopoMethod">Fast Metadata Check</span>
      </div>

      <div style="display: flex; justify-content: space-between; align-items: center; margin-top: 16px;">
        <span id="fileTopoSummary" style="font-size: 0.78rem; color: var(--ink-secondary); font-family: var(--font-mono);"></span>
        <div style="display: flex; gap: 8px;">
          <button class="btn" id="btnDeepCheck" onclick="inspectFileReplica(inspectingFilePath, true)">Deep Hash Verify</button>
          <button class="btn btn-primary" id="btnSyncSingleFile" onclick="syncCurrentFile()">Sync To Followers</button>
        </div>
      </div>
    </div>
  </div>

  <!-- Media Preview Modal -->
  <div class="backdrop" id="mediaModal">
    <div class="dialog-box" style="max-width: 90vw; max-height: 85vh; padding: 16px;">
      <div style="display: flex; justify-content: space-between; margin-bottom: 10px;">
        <span id="mediaTitle" style="font-weight: 500; font-size: 0.88rem;"></span>
        <button class="icon-btn" onclick="closeMediaModal()">✕</button>
      </div>
      <div id="mediaContainer" style="display: flex; justify-content: center; max-height: 75vh; overflow: auto;"></div>
    </div>
  </div>

  <!-- Toast -->
  <div class="toast" id="toast">
    <span id="toastMsg">Action completed</span>
  </div>

  <script>
    // App Architecture & State
    let currentPath = '';
    let allEntries = [];
    let filteredEntries = [];
    let renderedCount = 0;
    const CHUNK_SIZE = 40;
    let inspectingFilePath = '';
    // Cache for per-file replica state to avoid repeated probes: path -> { status, count, text }
    const replicaCache = new Map();

    let token = localStorage.getItem('ostore_token') || '';
    let eventSource = null;

    function getBasePath() {
      return window.location.pathname.startsWith('/ostore') ? '/ostore' : '';
    }

    const urlParams = new URLSearchParams(window.location.search);
    if (urlParams.has('token')) {
      token = urlParams.get('token');
      localStorage.setItem('ostore_token', token);
      window.history.replaceState({}, document.title, window.location.pathname);
    }

    function formatBytes(b) {
      if (!b || b === 0) return '0 B';
      const k = 1024, s = ['B', 'KB', 'MB', 'GB', 'TB'];
      const i = Math.floor(Math.log(b) / Math.log(k));
      return (b / Math.pow(k, i)).toFixed(1) + ' ' + s[i];
    }

    function formatDate(iso) {
      if (!iso) return '-';
      const d = new Date(iso);
      return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }

    function showToast(msg) {
      const t = document.getElementById('toast');
      document.getElementById('toastMsg').textContent = msg;
      t.style.display = 'flex';
      setTimeout(() => { t.style.display = 'none'; }, 2200);
    }

    async function api(path, opts = {}) {
      const url = `${getBasePath()}${path}`;
      opts.headers = opts.headers || {};
      if (token) opts.headers['Authorization'] = `Bearer ${token}`;
      return fetch(url, opts);
    }

    // Reactive SSE: Incremental updates without full-page redraws
    function initSSE() {
      if (eventSource) eventSource.close();
      const sseUrl = `${getBasePath()}/api/events${token ? `?token=${encodeURIComponent(token)}` : ''}`;
      eventSource = new EventSource(sseUrl);

      eventSource.onopen = () => {
        document.getElementById('sseDot').classList.remove('disconnected');
        document.getElementById('sseStatus').textContent = 'Live SSE Active';
      };

      eventSource.onerror = () => {
        document.getElementById('sseDot').classList.add('disconnected');
        document.getElementById('sseStatus').textContent = 'Reconnecting...';
      };

      eventSource.addEventListener('object.created', e => onObjectEvent(JSON.parse(e.data), 'created'));
      eventSource.addEventListener('object.updated', e => onObjectEvent(JSON.parse(e.data), 'updated'));
      eventSource.addEventListener('object.deleted', e => onObjectEvent(JSON.parse(e.data), 'deleted'));
      eventSource.addEventListener('object.moved', e => onObjectEvent(JSON.parse(e.data), 'moved'));
      eventSource.addEventListener('stream.reset', () => loadDirectory());
    }

    function onObjectEvent(data, kind) {
      const p = data.path;
      replicaCache.delete(p);
      const cur = currentPath ? currentPath + '/' : '';
      if (!p.startsWith(cur)) return;
      const sub = p.substring(cur.length);
      if (sub.includes('/')) return;

      const idx = allEntries.findIndex(e => e.name === sub);
      if (kind === 'deleted') {
        if (idx !== -1) {
          allEntries.splice(idx, 1);
          onFilter();
        }
      } else if (kind === 'created' || kind === 'updated') {
        const item = {
          name: sub,
          path: p,
          type: data.type || 'file',
          size: data.size || 0,
          mtime: data.ts || new Date().toISOString(),
          sha256: null
        };
        if (idx !== -1) allEntries[idx] = item;
        else allEntries.unshift(item);
        onFilter();
      }
    }

    // Load Topology: Main & Follower Graph
    async function loadTopology() {
      try {
        const res = await api('/api/cluster/topology');
        if (!res.ok) return;
        const topo = await res.json();

        // 1. Render SVG Topology Graph
        renderTopologySVG(topo);

        // 2. Render Main & Follower Cards
        const cardsList = document.getElementById('topoCardsList');
        const list = [];
        if (topo.control_plane) list.push({ ...topo.control_plane, role: 'Main Control Node' });
        for (const w of (topo.workers || [])) list.push({ ...w, role: 'Storage Follower' });

        cardsList.innerHTML = list.map(n => {
          const used = n.disk_total_bytes - n.disk_available_bytes;
          const pct = n.disk_total_bytes > 0 ? ((used / n.disk_total_bytes) * 100).toFixed(0) : 0;
          return `
            <div class="topo-card">
              <div class="topo-card-head">
                <span class="topo-card-title">
                  <span class="status-dot ${n.healthy ? '' : 'disconnected'}"></span>
                  ${n.name || n.id}
                </span>
                <span class="topo-card-badge">${n.role} · ${n.rtt_ms !== null ? n.rtt_ms + 'ms' : 'off'}</span>
              </div>
              <div class="topo-card-meta">
                <span>Used: ${pct}%</span>
                <span>Free: ${formatBytes(n.disk_available_bytes)}</span>
              </div>
              <div class="meter-bar">
                <div class="meter-fill" style="width: ${pct}%; background: ${n.is_local ? 'var(--accent)' : 'var(--success)'};"></div>
              </div>
            </div>
          `;
        }).join('');

        // 3. Render Distribution Spectrum
        const totalBytes = topo.distributions.reduce((acc, d) => acc + d.total_bytes, 0) || 1;
        const totalObj = topo.distributions.reduce((acc, d) => acc + d.object_count, 0);
        document.getElementById('spectrumSummaryText').textContent = `${totalObj} objects (${formatBytes(totalBytes)})`;
        document.getElementById('clusterCapacityText').textContent = `Total: ${formatBytes(topo.total_disk_bytes)} (Free: ${formatBytes(topo.available_disk_bytes)})`;

        const colors = ['#0071E3', '#34C759', '#FF9500', '#AF52DE', '#5856D6', '#FF2D55'];
        const bar = document.getElementById('spectrumBar');
        const legend = document.getElementById('spectrumLegend');

        bar.innerHTML = topo.distributions.map((d, i) => {
          const pct = ((d.total_bytes / totalBytes) * 100).toFixed(1);
          return `<div class="spectrum-segment" style="width: ${pct}%; background: ${colors[i % colors.length]};" title="${d.prefix}: ${formatBytes(d.total_bytes)} (${pct}%)"></div>`;
        }).join('');

        legend.innerHTML = topo.distributions.map((d, i) => `
          <div class="spectrum-legend-item">
            <span class="legend-dot" style="background: ${colors[i % colors.length]};"></span>
            <span>/${d.prefix} — ${formatBytes(d.total_bytes)} (${d.object_count} obj · ${d.replica_factor}x Synced)</span>
          </div>
        `).join('');

      } catch (e) {
        console.error('Topology error:', e);
      }
    }

    function renderTopologySVG(topo) {
      const svg = document.getElementById('topoCanvas');
      const cp = topo.control_plane;
      const workers = topo.workers || [];
      const w1 = workers[0] || { name: 'cc2-follower', rtt_ms: 1 };
      const w2 = workers[1] || { name: 'racknerd-follower', rtt_ms: 3 };

      svg.innerHTML = `
        <!-- Dynamic Links -->
        <path d="M270 65 Q210 115 150 160" fill="none" stroke="#D2D2D7" stroke-width="2" stroke-dasharray="4 4"/>
        <path d="M270 65 Q330 115 390 160" fill="none" stroke="#D2D2D7" stroke-width="2" stroke-dasharray="4 4"/>

        <!-- Main Node -->
        <g transform="translate(270, 45)">
          <circle r="24" fill="#FFFFFF" stroke="#0071E3" stroke-width="2.5"/>
          <circle r="4" fill="#34C759"/>
          <text y="-30" text-anchor="middle" fill="#1D1D1F" font-size="11" font-weight="600">${cp ? cp.name : 'Main'}</text>
          <text y="38" text-anchor="middle" fill="#86868B" font-size="9" font-family="monospace">Primary Entry (:8781)</text>
        </g>

        <!-- Follower 1 -->
        <g transform="translate(150, 165)">
          <circle r="20" fill="#FFFFFF" stroke="#34C759" stroke-width="2"/>
          <circle r="3" fill="#34C759"/>
          <text y="32" text-anchor="middle" fill="#1D1D1F" font-size="11" font-weight="500">${w1.name}</text>
          <text y="44" text-anchor="middle" fill="#86868B" font-size="9" font-family="monospace">${w1.rtt_ms}ms · Follower</text>
        </g>

        <!-- Follower 2 -->
        <g transform="translate(390, 165)">
          <circle r="20" fill="#FFFFFF" stroke="#34C759" stroke-width="2"/>
          <circle r="3" fill="#34C759"/>
          <text y="32" text-anchor="middle" fill="#1D1D1F" font-size="11" font-weight="500">${w2.name}</text>
          <text y="44" text-anchor="middle" fill="#86868B" font-size="9" font-family="monospace">${w2.rtt_ms}ms · Follower</text>
        </g>
      `;
    }

    // Object Explorer: Lazy Chunked Rendering (60fps)
    async function loadDirectory(path = currentPath) {
      currentPath = path;
      updateBreadcrumbs();
      try {
        const query = path ? `?path=${encodeURIComponent(path)}&checksums=1` : '?checksums=1';
        const res = await api(`/api/list${query}`);
        if (!res.ok) {
          if (res.status === 401) openTokenModal();
          return;
        }
        const data = await res.json();
        allEntries = data.entries || [];
        onFilter();
      } catch (e) {
        console.error('List error:', e);
      }
    }

    function onFilter() {
      const q = document.getElementById('searchBox').value.toLowerCase();
      filteredEntries = allEntries.filter(e => e.name.toLowerCase().includes(q));
      renderedCount = 0;
      document.getElementById('fileTableBody').innerHTML = '';
      renderNextBatch();
    }

    function renderNextBatch() {
      if (renderedCount >= filteredEntries.length) return;
      const batch = filteredEntries.slice(renderedCount, renderedCount + CHUNK_SIZE);
      const tbody = document.getElementById('fileTableBody');

      const rows = batch.map(item => {
        const isDir = item.type === 'dir';
        const dlUrl = `${getBasePath()}/api/files/${item.path}`;

        // Honest replica status: check cache or display truthful default
        let pillClass = 'probe-needed';
        let pillText = '● Main · Probe Followers';

        if (!isDir && replicaCache.has(item.path)) {
          const cached = replicaCache.get(item.path);
          pillClass = cached.pillClass;
          pillText = cached.pillText;
        }

        return `
          <tr class="file-row" id="row-${escapeId(item.path)}">
            <td>
              <div class="name-cell" onclick="${isDir ? `navigate('${item.path}')` : `openMediaPreview('${item.path}')`}">
                ${isDir
                  ? `<svg viewBox="0 0 24 24" fill="#0071E3"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>`
                  : `<svg viewBox="0 0 24 24" fill="none" stroke="#86868B" stroke-width="1.5"><path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>`
                }
                <span>${item.name}</span>
              </div>
            </td>
            <td class="meta-mono">${isDir ? '-' : formatBytes(item.size)}</td>
            <td class="meta-mono">${formatDate(item.mtime)}</td>
            <td>
              ${!isDir ? `
                <span class="replica-pill ${pillClass}" id="pill-${escapeId(item.path)}" onclick="inspectFileReplica('${item.path}')" title="Click to probe exact cluster replica topology">
                  <span>${pillText}</span>
                </span>
              ` : '-'}
            </td>
            <td>
              <div class="row-actions">
                ${!isDir ? `
                  <a class="icon-btn" href="${dlUrl}" download title="Download">
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                  </a>
                  <button class="icon-btn" title="Copy SHA-256" onclick="copySHA('${item.sha256}')">
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                  </button>
                ` : ''}
                <button class="icon-btn btn-delete" title="Delete" onclick="deleteItem('${item.path}', ${isDir})">
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                </button>
              </div>
            </td>
          </tr>
        `;
      }).join('');

      tbody.insertAdjacentHTML('beforeend', rows);
      renderedCount += batch.length;
    }

    function escapeId(str) {
      return btoa(unescape(encodeURIComponent(str))).replace(/[/+=]/g, '_');
    }

    const scrollObserver = new IntersectionObserver(entries => {
      if (entries[0].isIntersecting) renderNextBatch();
    }, { root: document.getElementById('tableViewport'), threshold: 0.1 });

    scrollObserver.observe(document.getElementById('scrollAnchor'));

    function navigate(p) {
      loadDirectory(p);
    }

    function updateBreadcrumbs() {
      const bc = document.getElementById('breadcrumbs');
      if (!currentPath) {
        bc.innerHTML = '<span style="font-weight: 600; color: var(--ink);">root</span>';
        return;
      }
      const parts = currentPath.split('/');
      let html = '<span class="crumb-link" onclick="navigate(\'\')">root</span>';
      let accum = '';
      for (let i = 0; i < parts.length; i++) {
        accum += (accum ? '/' : '') + parts[i];
        html += '<span class="crumb-sep">/</span>';
        if (i === parts.length - 1) {
          html += `<span style="font-weight: 600; color: var(--ink); padding: 3px 6px;">${parts[i]}</span>`;
        } else {
          html += `<span class="crumb-link" onclick="navigate('${accum}')">${parts[i]}</span>`;
        }
      }
      bc.innerHTML = html;
    }

    // Per-File Replica Topology Inspector (Honest & High-Performance)
    async function inspectFileReplica(filePath, deepVerify = false) {
      inspectingFilePath = filePath;
      document.getElementById('fileTopoPath').textContent = filePath;
      document.getElementById('fileTopoTitle').textContent = `Replica Topology: ${filePath.split('/').pop()}`;
      document.getElementById('fileTopoFlow').innerHTML = '<div style="text-align:center; padding: 20px; color: var(--ink-secondary);">Probing cluster nodes...</div>';
      document.getElementById('fileTopoMetrics').innerHTML = '<span>Probing...</span><span>--</span>';
      document.getElementById('fileTopoModal').style.display = 'flex';

      try {
        const query = deepVerify ? `?path=${encodeURIComponent(filePath)}&verify_hash=1` : `?path=${encodeURIComponent(filePath)}`;
        const res = await api(`/api/cluster/file-status${query}`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();

        const mainNode = data.main;
        const followers = data.followers || [];
        const metrics = data.metrics || {};

        let html = `
          <div class="file-node-row">
            <div>
              <div style="font-weight:600; font-size: 0.84rem; display: flex; align-items: center; gap: 6px;">
                <span class="status-dot ${mainNode.exists ? '' : 'disconnected'}"></span>
                <span>${mainNode.name}</span>
                <span class="topo-card-badge">Main Node (Control Plane)</span>
              </div>
              <div style="font-size: 0.72rem; font-family: var(--font-mono); color: var(--ink-secondary); margin-top: 4px;">
                ${mainNode.sha256 ? `SHA-256: ${mainNode.sha256.substring(0, 16)}...` : 'Status: Present on Main'}
              </div>
            </div>
            <div style="text-align: right; font-family: var(--font-mono); font-size: 0.78rem;">
              <div>${formatBytes(mainNode.size)}</div>
              <span class="replica-pill ${mainNode.exists ? 'synced' : 'main-only'}">${mainNode.exists ? 'Origin Present' : 'Missing'}</span>
            </div>
          </div>
        `;

        for (const f of followers) {
          const isMatched = f.exists && f.matches;
          html += `
            <div class="file-node-row">
              <div>
                <div style="font-weight:600; font-size: 0.84rem; display: flex; align-items: center; gap: 6px;">
                  <span class="status-dot ${isMatched ? '' : 'disconnected'}"></span>
                  <span>${f.name}</span>
                  <span class="topo-card-badge">Follower Node · ${f.rtt_ms !== null ? f.rtt_ms + 'ms' : 'off'}</span>
                </div>
                <div style="font-size: 0.72rem; font-family: var(--font-mono); color: var(--ink-secondary); margin-top: 4px;">
                  ${f.exists 
                    ? (f.sha256 ? `SHA-256: ${f.sha256.substring(0, 16)}...` : 'Matched Size & ETag') 
                    : '<strong style="color:var(--danger)">Missing on this node</strong>'}
                </div>
              </div>
              <div style="text-align: right; font-family: var(--font-mono); font-size: 0.78rem;">
                <div>${f.exists ? formatBytes(f.size) : '0 B'}</div>
                <span class="replica-pill ${isMatched ? 'synced' : (f.exists ? 'degraded' : 'main-only')}">
                  ${isMatched ? 'Replica Synced' : (f.exists ? 'Size Mismatch' : 'Not Replicated')}
                </span>
              </div>
            </div>
          `;
        }

        document.getElementById('fileTopoFlow').innerHTML = html;

        // Diagnostics metrics
        let timingBreakdown = `Total: ${metrics.total_ms}ms (Main: ${metrics.main_lookup_ms}ms`;
        for (const [k, v] of Object.entries(metrics.follower_timings || {})) {
          timingBreakdown += `, ${k}: ${v}ms`;
        }
        timingBreakdown += ')';
        document.getElementById('fileTopoMetrics').innerHTML = `
          <span>Probe Latency: <strong>${timingBreakdown}</strong></span>
          <span>Mode: <strong>${metrics.verify_hash ? 'Deep Hash' : 'Fast Tiered Probe'}</strong></span>
        `;

        // Update honest state pill in table row
        let pillClass = 'main-only';
        let pillText = `● Main Only (${data.replica_count}/${data.expected_replicas})`;
        if (data.fully_synced) {
          pillClass = 'synced';
          pillText = `● Synced (${data.replica_count}/${data.expected_replicas})`;
        } else if (data.replica_count > 1) {
          pillClass = 'degraded';
          pillText = `▲ Degraded (${data.replica_count}/${data.expected_replicas})`;
        }

        replicaCache.set(filePath, { pillClass, pillText });
        const rowPill = document.getElementById(`pill-${escapeId(filePath)}`);
        if (rowPill) {
          rowPill.className = `replica-pill ${pillClass}`;
          rowPill.innerHTML = `<span>${pillText}</span>`;
        }

        document.getElementById('fileTopoSummary').textContent = 
          `Status: ${data.replica_count}/${data.expected_replicas} Replicas Present`;

      } catch (e) {
        document.getElementById('fileTopoFlow').innerHTML = `<div style="color: var(--danger); padding: 14px;">Error querying replica: ${e}</div>`;
      }
    }

    function closeFileTopoModal() {
      document.getElementById('fileTopoModal').style.display = 'none';
    }

    async function syncCurrentFile() {
      if (!inspectingFilePath) return;
      showToast('Replicating to all missing followers...');
      const res = await api(`/api/cluster/sync-file?path=${encodeURIComponent(inspectingFilePath)}`, { method: 'POST' });
      if (res.ok) {
        showToast('File replicated successfully');
        inspectFileReplica(inspectingFilePath, false);
      }
    }

    // Media Preview: Guaranteed UTF-8 encoding (No Chinese Mojibake)
    async function openMediaPreview(path) {
      const ext = path.split('.').pop().toLowerCase();
      const url = `${getBasePath()}/api/files/${path}`;
      const modal = document.getElementById('mediaModal');
      const container = document.getElementById('mediaContainer');
      document.getElementById('mediaTitle').textContent = path.split('/').pop();

      if (['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp'].includes(ext)) {
        container.innerHTML = `<img src="${url}" style="max-width: 100%; max-height: 70vh; border-radius: 8px;">`;
      } else if (['mp4', 'webm', 'mov'].includes(ext)) {
        container.innerHTML = `<video src="${url}" controls autoplay style="max-width: 100%; max-height: 70vh; border-radius: 8px;"></video>`;
      } else if (['txt', 'json', 'log', 'md', 'xml', 'csv', 'sql'].includes(ext)) {
        try {
          const resp = await api(`/api/files/${path}`);
          const buf = await resp.arrayBuffer();
          // Explicit UTF-8 decoding avoids Latin-1 character mangling
          const text = new TextDecoder('utf-8').decode(buf);
          container.innerHTML = `<pre style="font-family: monospace; font-size: 0.8rem; background: #FAFAFC; padding: 14px; border-radius: 8px; width: 100%; max-height: 65vh; overflow: auto; color: #1D1D1F; white-space: pre-wrap;">${escapeHtml(text)}</pre>`;
        } catch (e) {
          container.innerHTML = `<div style="color: var(--danger)">Error reading file: ${e}</div>`;
        }
      } else {
        window.location.href = url;
        return;
      }
      modal.style.display = 'flex';
    }

    function escapeHtml(s) {
      return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    function closeMediaModal() {
      document.getElementById('mediaContainer').innerHTML = '';
      document.getElementById('mediaModal').style.display = 'none';
    }

    // Actions
    function openTokenModal() {
      document.getElementById('tokenInput').value = token;
      document.getElementById('tokenModal').style.display = 'flex';
    }
    function closeTokenModal() { document.getElementById('tokenModal').style.display = 'none'; }
    function saveToken() {
      token = document.getElementById('tokenInput').value.trim();
      localStorage.setItem('ostore_token', token);
      closeTokenModal();
      initSSE();
      loadTopology();
      loadDirectory();
    }

    function copySHA(sha) {
      if (!sha) return;
      navigator.clipboard.writeText(sha);
      showToast('SHA-256 copied to clipboard');
    }

    async function deleteItem(path, isDir) {
      if (!confirm(`Delete ${path}?`)) return;
      const res = await api(`/api/files/${path}${isDir ? '?recursive=1' : ''}`, { method: 'DELETE' });
      if (res.ok) showToast('Object deleted');
    }

    async function openMkdir() {
      const name = prompt('Folder name:');
      if (!name) return;
      const target = currentPath ? `${currentPath}/${name}` : name;
      const res = await api('/api/mkdir', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: target })
      });
      if (res.ok) showToast('Folder created');
    }

    function triggerFileInput() { document.getElementById('fileInput').click(); }
    async function handleFileSelect(e) {
      const files = Array.from(e.target.files);
      showToast(`Uploading ${files.length} file(s)...`);
      for (const f of files) {
        const dest = currentPath ? `${currentPath}/${f.name}` : f.name;
        await api(`/api/files/${dest}`, { method: 'PUT', body: f });
      }
      showToast('Uploads completed');
      e.target.value = '';
    }

    async function triggerSync() {
      if (!confirm('Run cluster replication sync across all follower nodes?')) return;
      showToast('Cluster synchronization started...');
      const res = await api('/api/cluster/sync', { method: 'POST' });
      if (res.ok) {
        const r = await res.json();
        showToast(`Sync finished in ${r.duration_ms}ms`);
        loadTopology();
      }
    }

    // Start
    initSSE();
    loadTopology();
    loadDirectory();
  </script>
</body>
</html>
"##;
