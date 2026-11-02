//! Modern Single-Page Application (SPA) Web UI for ostore.
//!
//! Embedded directly into the binary with zero runtime external dependencies.
//! Provides storage browsing, drag-and-drop upload, cluster status, media preview,
//! and automatic base-path adaptation (supporting both `/` and `/ostore`).

use axum::response::Html;

pub async fn handler() -> Html<&'static str> {
    Html(UI_HTML)
}

const UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>ostore - Object Storage</title>
  <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='%236366f1'><path d='M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5'/></svg>">
  <style>
    :root {
      --bg: #090d16;
      --card-bg: rgba(22, 29, 47, 0.75);
      --card-border: rgba(255, 255, 255, 0.08);
      --card-hover: rgba(30, 41, 67, 0.85);
      --text-main: #f1f5f9;
      --text-muted: #94a3b8;
      --accent: #6366f1;
      --accent-hover: #4f46e5;
      --accent-glow: rgba(99, 102, 241, 0.25);
      --success: #10b981;
      --success-glow: rgba(16, 185, 129, 0.2);
      --danger: #ef4444;
      --warning: #f59e0b;
      --border-radius: 14px;
      --font: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    }

    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: var(--font);
      background-color: var(--bg);
      color: var(--text-main);
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      background-image: 
        radial-gradient(at 0% 0%, rgba(99, 102, 241, 0.12) 0px, transparent 50%),
        radial-gradient(at 100% 100%, rgba(16, 185, 129, 0.08) 0px, transparent 50%);
      background-attachment: fixed;
    }

    /* Container */
    .container {
      max-width: 1240px;
      margin: 0 auto;
      padding: 24px 20px;
      width: 100%;
      flex: 1;
    }

    /* Header */
    header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-bottom: 24px;
      border-bottom: 1px solid var(--card-border);
      margin-bottom: 24px;
      flex-wrap: wrap;
      gap: 16px;
    }

    .brand {
      display: flex;
      align-items: center;
      gap: 12px;
      text-decoration: none;
      color: var(--text-main);
    }

    .brand-icon {
      width: 38px;
      height: 38px;
      background: linear-gradient(135deg, #6366f1, #3b82f6);
      border-radius: 10px;
      display: flex;
      align-items: center;
      justify-content: center;
      box-shadow: 0 4px 12px var(--accent-glow);
    }

    .brand-icon svg { width: 22px; height: 22px; fill: white; }
    .brand-title { font-size: 1.35rem; font-weight: 700; letter-spacing: -0.5px; }
    .brand-subtitle { font-size: 0.75rem; color: var(--text-muted); font-weight: 500; }

    .header-actions {
      display: flex;
      align-items: center;
      gap: 12px;
      flex-wrap: wrap;
    }

    .badge {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 6px 12px;
      border-radius: 999px;
      font-size: 0.78rem;
      font-weight: 600;
      background: rgba(255, 255, 255, 0.06);
      border: 1px solid var(--card-border);
    }

    .badge-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background-color: var(--success);
      box-shadow: 0 0 8px var(--success);
    }

    .btn {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 8px 16px;
      border-radius: 10px;
      font-size: 0.85rem;
      font-weight: 600;
      cursor: pointer;
      border: 1px solid var(--card-border);
      background: var(--card-bg);
      color: var(--text-main);
      transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
      backdrop-filter: blur(12px);
    }

    .btn:hover {
      background: var(--card-hover);
      transform: translateY(-1px);
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
    }

    .btn-primary {
      background: linear-gradient(135deg, var(--accent), #4f46e5);
      border-color: rgba(255, 255, 255, 0.15);
      color: white;
      box-shadow: 0 4px 14px var(--accent-glow);
    }

    .btn-primary:hover {
      background: linear-gradient(135deg, #4f46e5, #4338ca);
      box-shadow: 0 6px 20px var(--accent-glow);
    }

    /* Cluster Status Card */
    .cluster-card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--border-radius);
      padding: 20px;
      margin-bottom: 24px;
      backdrop-filter: blur(12px);
      display: flex;
      flex-direction: column;
      gap: 16px;
      box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
    }

    .cluster-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
    }

    .cluster-title {
      font-size: 1rem;
      font-weight: 700;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .progress-bar-container {
      width: 100%;
      height: 8px;
      background: rgba(255, 255, 255, 0.08);
      border-radius: 999px;
      overflow: hidden;
    }

    .progress-bar-fill {
      height: 100%;
      background: linear-gradient(90deg, #6366f1, #10b981);
      border-radius: 999px;
      width: 0%;
      transition: width 0.6s ease;
    }

    .nodes-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 14px;
    }

    .node-item {
      background: rgba(255, 255, 255, 0.03);
      border: 1px solid var(--card-border);
      border-radius: 10px;
      padding: 14px;
      display: flex;
      flex-direction: column;
      gap: 8px;
      transition: border-color 0.2s ease;
    }

    .node-item:hover {
      border-color: rgba(99, 102, 241, 0.4);
    }

    .node-item-top {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }

    .node-name { font-weight: 600; font-size: 0.88rem; }
    .node-tag {
      font-size: 0.7rem;
      padding: 2px 6px;
      border-radius: 4px;
      background: rgba(99, 102, 241, 0.15);
      color: #a5b4fc;
    }

    .node-metrics {
      font-size: 0.78rem;
      color: var(--text-muted);
      display: flex;
      justify-content: space-between;
    }

    /* Main Explorer Card */
    .explorer-card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--border-radius);
      overflow: hidden;
      backdrop-filter: blur(12px);
      box-shadow: 0 8px 30px rgba(0, 0, 0, 0.25);
    }

    /* Explorer Controls */
    .explorer-controls {
      padding: 16px 20px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      border-bottom: 1px solid var(--card-border);
      flex-wrap: wrap;
      gap: 14px;
    }

    .breadcrumbs {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 0.9rem;
      flex-wrap: wrap;
    }

    .breadcrumb-item {
      color: var(--accent);
      cursor: pointer;
      text-decoration: none;
      font-weight: 500;
      padding: 2px 6px;
      border-radius: 6px;
      transition: background 0.15s;
    }

    .breadcrumb-item:hover {
      background: rgba(99, 102, 241, 0.12);
    }

    .breadcrumb-separator {
      color: var(--text-muted);
      font-size: 0.8rem;
    }

    .explorer-actions {
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
    }

    .search-box {
      position: relative;
      display: flex;
      align-items: center;
    }

    .search-input {
      background: rgba(0, 0, 0, 0.25);
      border: 1px solid var(--card-border);
      border-radius: 8px;
      padding: 7px 12px 7px 32px;
      color: var(--text-main);
      font-size: 0.85rem;
      outline: none;
      transition: all 0.2s;
      width: 180px;
    }

    .search-input:focus {
      width: 240px;
      border-color: var(--accent);
      box-shadow: 0 0 0 2px var(--accent-glow);
    }

    .search-icon {
      position: absolute;
      left: 10px;
      width: 14px;
      height: 14px;
      fill: var(--text-muted);
      pointer-events: none;
    }

    /* Dropzone area */
    .dropzone-overlay {
      position: relative;
    }

    .dropzone-overlay.drag-over::after {
      content: "Drop files here to upload";
      position: absolute;
      inset: 0;
      background: rgba(15, 23, 42, 0.92);
      border: 2px dashed var(--accent);
      border-radius: var(--border-radius);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 1.25rem;
      font-weight: 600;
      color: var(--accent);
      z-index: 50;
      backdrop-filter: blur(4px);
    }

    /* File Table */
    .file-table {
      width: 100%;
      border-collapse: collapse;
      text-align: left;
      font-size: 0.88rem;
    }

    .file-table th {
      padding: 12px 20px;
      color: var(--text-muted);
      font-weight: 600;
      font-size: 0.78rem;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      border-bottom: 1px solid var(--card-border);
    }

    .file-table tr {
      border-bottom: 1px solid rgba(255, 255, 255, 0.04);
      transition: background 0.15s ease;
    }

    .file-table tbody tr:hover {
      background: var(--card-hover);
    }

    .file-table td {
      padding: 12px 20px;
      vertical-align: middle;
    }

    .file-name-cell {
      display: flex;
      align-items: center;
      gap: 12px;
      font-weight: 500;
      cursor: pointer;
    }

    .file-icon {
      width: 24px;
      height: 24px;
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
    }

    .file-icon svg { width: 20px; height: 20px; }

    .sha-badge {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      font-size: 0.75rem;
      padding: 2px 6px;
      background: rgba(255, 255, 255, 0.05);
      border-radius: 4px;
      color: var(--text-muted);
      cursor: pointer;
      transition: all 0.2s;
    }

    .sha-badge:hover {
      background: rgba(99, 102, 241, 0.2);
      color: #c7d2fe;
    }

    .table-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      justify-content: flex-end;
    }

    .action-btn {
      background: transparent;
      border: none;
      cursor: pointer;
      padding: 6px;
      border-radius: 6px;
      color: var(--text-muted);
      transition: all 0.15s;
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .action-btn:hover {
      background: rgba(255, 255, 255, 0.1);
      color: var(--text-main);
    }

    .action-btn.btn-delete:hover {
      background: rgba(239, 68, 68, 0.2);
      color: var(--danger);
    }

    .empty-state {
      padding: 48px 20px;
      text-align: center;
      color: var(--text-muted);
    }

    /* Upload Task Drawer / Floating Toast */
    .upload-toast-panel {
      position: fixed;
      bottom: 24px;
      right: 24px;
      width: 340px;
      background: #111827;
      border: 1px solid var(--card-border);
      border-radius: 12px;
      box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4);
      z-index: 100;
      padding: 16px;
      display: none;
      flex-direction: column;
      gap: 10px;
    }

    .upload-toast-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      font-size: 0.85rem;
      font-weight: 600;
    }

    .upload-item {
      display: flex;
      flex-direction: column;
      gap: 4px;
      font-size: 0.78rem;
    }

    .upload-item-name {
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .upload-progress {
      height: 4px;
      background: rgba(255, 255, 255, 0.1);
      border-radius: 999px;
      overflow: hidden;
    }

    .upload-progress-fill {
      height: 100%;
      background: var(--accent);
      width: 0%;
      transition: width 0.2s;
    }

    /* Modal / Dialog */
    .modal-overlay {
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.75);
      backdrop-filter: blur(6px);
      display: none;
      align-items: center;
      justify-content: center;
      z-index: 200;
      padding: 20px;
    }

    .modal-content {
      background: #131c31;
      border: 1px solid var(--card-border);
      border-radius: var(--border-radius);
      padding: 24px;
      width: 100%;
      max-width: 520px;
      box-shadow: 0 20px 40px rgba(0, 0, 0, 0.5);
      display: flex;
      flex-direction: column;
      gap: 16px;
    }

    .modal-title { font-size: 1.15rem; font-weight: 700; }
    .modal-input {
      width: 100%;
      background: rgba(0, 0, 0, 0.3);
      border: 1px solid var(--card-border);
      border-radius: 8px;
      padding: 10px 14px;
      color: var(--text-main);
      font-size: 0.9rem;
      outline: none;
    }
    .modal-input:focus { border-color: var(--accent); }

    .modal-actions {
      display: flex;
      justify-content: flex-end;
      gap: 10px;
    }

    /* Media Preview Modal */
    .preview-modal-content {
      max-width: 90vw;
      max-height: 90vh;
      width: auto;
      background: #0f172a;
      padding: 16px;
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 12px;
    }

    .preview-media-container {
      max-width: 85vw;
      max-height: 75vh;
      overflow: auto;
      display: flex;
      justify-content: center;
    }

    .preview-media-container img {
      max-width: 100%;
      max-height: 75vh;
      object-fit: contain;
      border-radius: 8px;
    }

    .preview-media-container video {
      max-width: 100%;
      max-height: 75vh;
      border-radius: 8px;
      outline: none;
    }

    .preview-media-container pre {
      background: #050811;
      padding: 16px;
      border-radius: 8px;
      font-family: ui-monospace, SFMono-Regular, monospace;
      font-size: 0.85rem;
      white-space: pre-wrap;
      max-height: 70vh;
      overflow-y: auto;
      width: 80vw;
      color: #e2e8f0;
    }

    /* Toast */
    .toast-msg {
      position: fixed;
      bottom: 24px;
      left: 50%;
      transform: translateX(-50%);
      background: rgba(16, 185, 129, 0.95);
      color: white;
      padding: 8px 18px;
      border-radius: 999px;
      font-size: 0.85rem;
      font-weight: 600;
      box-shadow: 0 4px 14px rgba(0, 0, 0, 0.3);
      display: none;
      z-index: 300;
    }
  </style>
</head>
<body>
  <div class="container dropzone-overlay" id="mainDropzone">
    <!-- Header -->
    <header>
      <a href="#" class="brand" onclick="navigate(''); return false;">
        <div class="brand-icon">
          <svg viewBox="0 0 24 24"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>
        </div>
        <div>
          <div class="brand-title">ostore</div>
          <div class="brand-subtitle">Distributed Object Storage Control Plane</div>
        </div>
      </a>

      <div class="header-actions">
        <div class="badge" id="clusterBadge">
          <span class="badge-dot"></span>
          <span id="clusterBadgeText">Checking cluster...</span>
        </div>
        <button class="btn" onclick="openTokenModal()">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
          <span id="tokenBtnText">Set Token</span>
        </button>
        <button class="btn" onclick="refreshAll()">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 4v6h-6"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>
          Refresh
        </button>
      </div>
    </header>

    <!-- Cluster Overview Card -->
    <section class="cluster-card" id="clusterCard">
      <div class="cluster-header">
        <div class="cluster-title">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="2" width="20" height="8" rx="2" ry="2"/><rect x="2" y="14" width="20" height="8" rx="2" ry="2"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="18" x2="6.01" y2="18"/></svg>
          Storage Fleet & Node Capacity
        </div>
        <div style="display: flex; gap: 8px; align-items: center;">
          <span id="clusterDiskText" style="font-size: 0.85rem; color: var(--text-muted);">Capacity loading...</span>
          <button class="btn btn-primary" style="padding: 6px 12px; font-size: 0.78rem;" onclick="syncClusterBackups()">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
            Sync ClipVault Replicas
          </button>
        </div>
      </div>
      <div class="progress-bar-container">
        <div class="progress-bar-fill" id="clusterDiskBar"></div>
      </div>
      <div class="nodes-grid" id="nodesGrid">
        <!-- Node items injected dynamically -->
      </div>
    </section>

    <!-- Explorer Card -->
    <section class="explorer-card">
      <div class="explorer-controls">
        <div class="breadcrumbs" id="breadcrumbs">
          <a class="breadcrumb-item" href="#" onclick="navigate(''); return false;">root</a>
        </div>
        <div class="explorer-actions">
          <div class="search-box">
            <svg class="search-icon" viewBox="0 0 24 24"><path d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>
            <input type="text" class="search-input" id="filterInput" placeholder="Filter files..." oninput="onFilterChanged()">
          </div>
          <button class="btn" onclick="openMkdirModal()">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="12" y1="11" x2="12" y2="17"/><line x1="9" y1="14" x2="15" y2="14"/></svg>
            New Folder
          </button>
          <button class="btn btn-primary" onclick="triggerFileInput()">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
            Upload
          </button>
          <input type="file" id="fileUploadInput" multiple style="display: none;" onchange="handleFileSelect(event)">
        </div>
      </div>

      <table class="file-table">
        <thead>
          <tr>
            <th style="width: 42%;">Name</th>
            <th style="width: 14%;">Size</th>
            <th style="width: 18%;">Modified</th>
            <th style="width: 16%;">SHA-256</th>
            <th style="text-align: right; width: 10%;">Actions</th>
          </tr>
        </thead>
        <tbody id="fileTableBody">
          <!-- Files injected dynamically -->
        </tbody>
      </table>
      <div class="empty-state" id="emptyState" style="display: none;">
        No items in this directory. Drag and drop files to upload.
      </div>
    </section>
  </div>

  <!-- Upload Task Toast -->
  <div class="upload-toast-panel" id="uploadToast">
    <div class="upload-toast-header">
      <span id="uploadToastTitle">Uploading 0 files</span>
      <button class="action-btn" onclick="closeUploadToast()">✕</button>
    </div>
    <div id="uploadItemsContainer" style="display: flex; flex-direction: column; gap: 8px; max-height: 180px; overflow-y: auto;"></div>
  </div>

  <!-- Token Modal -->
  <div class="modal-overlay" id="tokenModal">
    <div class="modal-content">
      <div class="modal-title">Set API Access Token</div>
      <p style="font-size: 0.85rem; color: var(--text-muted);">
        Enter your ostore Bearer token. It will be stored securely in your browser's local storage.
      </p>
      <input type="password" class="modal-input" id="tokenInput" placeholder="Enter Bearer Token">
      <div class="modal-actions">
        <button class="btn" onclick="closeTokenModal()">Cancel</button>
        <button class="btn btn-primary" onclick="saveToken()">Save Token</button>
      </div>
    </div>
  </div>

  <!-- Mkdir Modal -->
  <div class="modal-overlay" id="mkdirModal">
    <div class="modal-content">
      <div class="modal-title">Create New Folder</div>
      <input type="text" class="modal-input" id="mkdirInput" placeholder="Folder name">
      <div class="modal-actions">
        <button class="btn" onclick="closeMkdirModal()">Cancel</button>
        <button class="btn btn-primary" onclick="submitMkdir()">Create</button>
      </div>
    </div>
  </div>

  <!-- Media Preview Modal -->
  <div class="modal-overlay" id="previewModal">
    <div class="modal-content preview-modal-content">
      <div style="display: flex; justify-content: space-between; width: 100%; align-items: center;">
        <span class="modal-title" id="previewTitle">File Preview</span>
        <button class="action-btn" onclick="closePreviewModal()">✕</button>
      </div>
      <div class="preview-media-container" id="previewContainer"></div>
    </div>
  </div>

  <!-- Toast -->
  <div class="toast-msg" id="toastMsg">Action completed</div>

  <script>
    // State
    let currentPath = '';
    let currentEntries = [];
    let token = localStorage.getItem('ostore_token') || '';

    // Auto-detect base path: support /ostore or /
    function getBasePath() {
      const p = window.location.pathname;
      if (p.startsWith('/ostore')) return '/ostore';
      return '';
    }

    // Check URL token query
    const urlParams = new URLSearchParams(window.location.search);
    if (urlParams.has('token')) {
      token = urlParams.get('token');
      localStorage.setItem('ostore_token', token);
      // Clean query from address bar
      window.history.replaceState({}, document.title, window.location.pathname);
    }

    function updateTokenUI() {
      const btnText = document.getElementById('tokenBtnText');
      if (token) {
        btnText.textContent = 'Token Active';
        btnText.style.color = '#34d399';
      } else {
        btnText.textContent = 'Set Token';
        btnText.style.color = 'var(--text-main)';
      }
    }

    function openTokenModal() {
      document.getElementById('tokenInput').value = token;
      document.getElementById('tokenModal').style.display = 'flex';
    }
    function closeTokenModal() { document.getElementById('tokenModal').style.display = 'none'; }
    function saveToken() {
      token = document.getElementById('tokenInput').value.trim();
      localStorage.setItem('ostore_token', token);
      closeTokenModal();
      updateTokenUI();
      refreshAll();
    }

    function showToast(msg) {
      const t = document.getElementById('toastMsg');
      t.textContent = msg;
      t.style.display = 'block';
      setTimeout(() => { t.style.display = 'none'; }, 2500);
    }

    function formatBytes(bytes) {
      if (bytes === 0 || !bytes) return '0 B';
      const k = 1024;
      const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
      const i = Math.floor(Math.log(bytes) / Math.log(k));
      return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    }

    function formatDate(iso) {
      if (!iso) return '-';
      const d = new Date(iso);
      return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }

    // API Helper
    async function apiFetch(endpoint, options = {}) {
      const base = getBasePath();
      const url = `${base}${endpoint}`;
      options.headers = options.headers || {};
      if (token) {
        options.headers['Authorization'] = `Bearer ${token}`;
      }
      return fetch(url, options);
    }

    // Load Cluster Fleet Status
    async function loadClusterStatus() {
      try {
        const res = await apiFetch('/api/cluster/nodes');
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();
        
        const nodes = data.nodes || [];
        const healthyCount = nodes.filter(n => n.healthy).length;
        document.getElementById('clusterBadgeText').textContent = `Cluster: ${healthyCount}/${nodes.length} Nodes Active`;

        const total = data.total_disk_bytes || 0;
        const avail = data.available_disk_bytes || 0;
        const used = total - avail;
        const pct = total > 0 ? ((used / total) * 100).toFixed(1) : 0;

        document.getElementById('clusterDiskText').textContent = 
          `Used ${formatBytes(used)} / ${formatBytes(total)} (${pct}%)`;
        document.getElementById('clusterDiskBar').style.width = `${pct}%`;

        const grid = document.getElementById('nodesGrid');
        grid.innerHTML = nodes.map(n => `
          <div class="node-item">
            <div class="node-item-top">
              <span class="node-name">${n.name || n.id}</span>
              <span class="node-tag">${n.is_local ? 'Local Master' : 'Storage Worker'}</span>
            </div>
            <div class="node-metrics">
              <span>Status: <strong style="color: ${n.healthy ? 'var(--success)' : 'var(--danger)'}">${n.healthy ? 'Online' : 'Offline'}</strong></span>
              <span>RTT: ${n.rtt_ms !== null ? n.rtt_ms + ' ms' : '-'}</span>
            </div>
            <div class="node-metrics">
              <span>Free: ${formatBytes(n.disk_available_bytes)}</span>
              <span>Total: ${formatBytes(n.disk_total_bytes)}</span>
            </div>
          </div>
        `).join('');
      } catch (e) {
        document.getElementById('clusterBadgeText').textContent = 'Standalone Mode';
      }
    }

    // Load Directory Listing
    async function loadDirectory(path = currentPath) {
      currentPath = path;
      updateBreadcrumbs();
      try {
        const query = path ? `?path=${encodeURIComponent(path)}&checksums=1` : '?checksums=1';
        const res = await apiFetch(`/api/list${query}`);
        if (!res.ok) {
          if (res.status === 401) {
            openTokenModal();
            return;
          }
          throw new Error(`HTTP ${res.status}`);
        }
        const data = await res.json();
        currentEntries = data.entries || [];
        renderTable(currentEntries);
      } catch (e) {
        console.error('List error:', e);
        document.getElementById('fileTableBody').innerHTML = `
          <tr><td colspan="5" style="text-align:center; color: var(--danger); padding: 30px;">
            Failed to load directory. Check API token.
          </td></tr>
        `;
      }
    }

    function renderTable(entries) {
      const tbody = document.getElementById('fileTableBody');
      const empty = document.getElementById('emptyState');
      if (!entries || entries.length === 0) {
        tbody.innerHTML = '';
        empty.style.display = 'block';
        return;
      }
      empty.style.display = 'none';

      tbody.innerHTML = entries.map(item => {
        const isDir = item.type === 'dir';
        const iconSvg = getIconSvg(item.name, isDir);
        const shaShort = item.sha256 ? item.sha256.substring(0, 8) : '';
        const dlUrl = `${getBasePath()}/api/files/${item.path}`;

        return `
          <tr>
            <td>
              <div class="file-name-cell" onclick="${isDir ? `navigate('${item.path}')` : `previewOrDownload('${item.path}')`}">
                <div class="file-icon">${iconSvg}</div>
                <span title="${item.name}">${item.name}</span>
              </div>
            </td>
            <td style="color: var(--text-muted);">${isDir ? '-' : formatBytes(item.size)}</td>
            <td style="color: var(--text-muted);">${formatDate(item.mtime)}</td>
            <td>
              ${shaShort ? `
                <span class="sha-badge" title="Full SHA-256: ${item.sha256}\nClick to copy" onclick="copyToClipboard('${item.sha256}')">
                  ${shaShort}
                </span>` : '-'}
            </td>
            <td>
              <div class="table-actions">
                ${!isDir ? `
                  <a class="action-btn" href="${dlUrl}" download title="Download" onclick="event.stopPropagation()">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                  </a>
                  <button class="action-btn" title="Copy direct link" onclick="copyLink('${dlUrl}'); event.stopPropagation()">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
                  </button>
                ` : ''}
                <button class="action-btn btn-delete" title="Delete" onclick="deleteItem('${item.path}', ${isDir}); event.stopPropagation()">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                </button>
              </div>
            </td>
          </tr>
        `;
      }).join('');
    }

    function getIconSvg(name, isDir) {
      if (isDir) {
        return `<svg viewBox="0 0 24 24" fill="#38bdf8"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>`;
      }
      const ext = name.split('.').pop().toLowerCase();
      if (['mp4', 'mkv', 'webm', 'mov', 'avi'].includes(ext)) {
        return `<svg viewBox="0 0 24 24" fill="#f43f5e"><polygon points="5 3 19 12 5 21 5 3"/></svg>`;
      }
      if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp'].includes(ext)) {
        return `<svg viewBox="0 0 24 24" fill="#10b981"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>`;
      }
      if (['db', 'sqlite', 'sqlite3'].includes(ext)) {
        return `<svg viewBox="0 0 24 24" fill="#eab308"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/></svg>`;
      }
      return `<svg viewBox="0 0 24 24" fill="#94a3b8"><path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>`;
    }

    function navigate(path) {
      loadDirectory(path);
    }

    function updateBreadcrumbs() {
      const bc = document.getElementById('breadcrumbs');
      if (!currentPath) {
        bc.innerHTML = '<span class="breadcrumb-item" style="color: var(--text-main);">root</span>';
        return;
      }
      const parts = currentPath.split('/');
      let html = '<a class="breadcrumb-item" href="#" onclick="navigate(\'\'); return false;">root</a>';
      let accum = '';
      for (let i = 0; i < parts.length; i++) {
        accum += (accum ? '/' : '') + parts[i];
        html += '<span class="breadcrumb-separator">/</span>';
        if (i === parts.length - 1) {
          html += `<span class="breadcrumb-item" style="color: var(--text-main);">${parts[i]}</span>`;
        } else {
          const p = accum;
          html += `<a class="breadcrumb-item" href="#" onclick="navigate('${p}'); return false;">${parts[i]}</a>`;
        }
      }
      bc.innerHTML = html;
    }

    function onFilterChanged() {
      const val = document.getElementById('filterInput').value.toLowerCase();
      const filtered = currentEntries.filter(e => e.name.toLowerCase().includes(val));
      renderTable(filtered);
    }

    // Modal Actions
    function openMkdirModal() {
      document.getElementById('mkdirInput').value = '';
      document.getElementById('mkdirModal').style.display = 'flex';
    }
    function closeMkdirModal() { document.getElementById('mkdirModal').style.display = 'none'; }
    async function submitMkdir() {
      const name = document.getElementById('mkdirInput').value.trim();
      if (!name) return;
      const target = currentPath ? `${currentPath}/${name}` : name;
      try {
        const res = await apiFetch('/api/mkdir', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: target })
        });
        if (res.ok) {
          closeMkdirModal();
          showToast(`Directory "${name}" created`);
          loadDirectory();
        } else {
          alert('Failed to create directory');
        }
      } catch (e) {
        alert('Error: ' + e);
      }
    }

    async function deleteItem(path, isDir) {
      if (!confirm(`Are you sure you want to delete "${path}"?`)) return;
      try {
        const query = isDir ? '?recursive=1' : '';
        const res = await apiFetch(`/api/files/${path}${query}`, { method: 'DELETE' });
        if (res.ok) {
          showToast(`Deleted ${path}`);
          loadDirectory();
        } else {
          alert('Delete failed');
        }
      } catch (e) {
        alert('Error: ' + e);
      }
    }

    // File Upload Handling
    function triggerFileInput() {
      document.getElementById('fileUploadInput').click();
    }

    function handleFileSelect(e) {
      const files = Array.from(e.target.files);
      uploadFiles(files);
      e.target.value = '';
    }

    // Drag and Drop
    const dropzone = document.getElementById('mainDropzone');
    dropzone.addEventListener('dragover', (e) => {
      e.preventDefault();
      dropzone.classList.add('drag-over');
    });
    dropzone.addEventListener('dragleave', (e) => {
      e.preventDefault();
      dropzone.classList.remove('drag-over');
    });
    dropzone.addEventListener('drop', (e) => {
      e.preventDefault();
      dropzone.classList.remove('drag-over');
      if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
        uploadFiles(Array.from(e.dataTransfer.files));
      }
    });

    async function uploadFiles(files) {
      if (!files.length) return;
      const toast = document.getElementById('uploadToast');
      const title = document.getElementById('uploadToastTitle');
      const container = document.getElementById('uploadItemsContainer');

      toast.style.display = 'flex';
      title.textContent = `Uploading ${files.length} file(s)...`;
      container.innerHTML = '';

      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        const itemDiv = document.createElement('div');
        itemDiv.className = 'upload-item';
        itemDiv.innerHTML = `
          <div class="upload-item-name">${file.name}</div>
          <div class="upload-progress">
            <div class="upload-progress-fill" id="prog-${i}"></div>
          </div>
        `;
        container.appendChild(itemDiv);

        const targetPath = currentPath ? `${currentPath}/${file.name}` : file.name;
        try {
          const res = await apiFetch(`/api/files/${targetPath}`, {
            method: 'PUT',
            body: file
          });
          const bar = document.getElementById(`prog-${i}`);
          if (res.ok) {
            bar.style.width = '100%';
            bar.style.background = 'var(--success)';
          } else {
            bar.style.width = '100%';
            bar.style.background = 'var(--danger)';
          }
        } catch (e) {
          console.error('Upload failed:', e);
        }
      }

      title.textContent = 'Uploads finished';
      showToast('All files uploaded');
      loadDirectory();
    }

    function closeUploadToast() {
      document.getElementById('uploadToast').style.display = 'none';
    }

    // Media Preview Modal
    async function previewOrDownload(path) {
      const ext = path.split('.').pop().toLowerCase();
      const base = getBasePath();
      const fullUrl = `${base}/api/files/${path}`;

      const modal = document.getElementById('previewModal');
      const title = document.getElementById('previewTitle');
      const container = document.getElementById('previewContainer');
      title.textContent = path.split('/').pop();

      if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp'].includes(ext)) {
        container.innerHTML = `<img src="${fullUrl}" alt="${path}">`;
        modal.style.display = 'flex';
      } else if (['mp4', 'webm', 'mov'].includes(ext)) {
        container.innerHTML = `<video src="${fullUrl}" controls autoplay></video>`;
        modal.style.display = 'flex';
      } else if (['txt', 'json', 'log', 'md', 'xml'].includes(ext)) {
        try {
          const res = await apiFetch(`/api/files/${path}`);
          const text = await res.text();
          container.innerHTML = `<pre>${escapeHtml(text)}</pre>`;
          modal.style.display = 'flex';
        } catch (e) {
          window.location.href = fullUrl;
        }
      } else {
        window.location.href = fullUrl;
      }
    }

    function closePreviewModal() {
      const container = document.getElementById('previewContainer');
      container.innerHTML = '';
      document.getElementById('previewModal').style.display = 'none';
    }

    function escapeHtml(str) {
      return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    function copyToClipboard(text) {
      navigator.clipboard.writeText(text).then(() => {
        showToast('SHA-256 copied to clipboard');
      });
    }

    function copyLink(url) {
      const full = window.location.origin + url;
      navigator.clipboard.writeText(full).then(() => {
        showToast('Link copied to clipboard');
      });
    }

    // Cluster Backup Replication Sync
    async function syncClusterBackups() {
      if (!confirm('Run cluster replica sync for "clipvault"? This checks missing files across all storage nodes.')) return;
      showToast('Cluster synchronization started...');
      try {
        const res = await apiFetch('/api/cluster/sync?prefix=clipvault', { method: 'POST' });
        if (res.ok) {
          const rep = await res.json();
          let msg = `Sync completed in ${rep.duration_ms}ms: `;
          for (const nr of (rep.node_results || [])) {
            msg += `[Node ${nr.node_id}: Synced ${nr.synced}, Skipped ${nr.skipped}] `;
          }
          alert(msg);
          loadClusterStatus();
        } else {
          alert('Sync failed or cluster mode not active');
        }
      } catch (e) {
        alert('Sync error: ' + e);
      }
    }

    function refreshAll() {
      loadClusterStatus();
      loadDirectory();
    }

    // Init
    updateTokenUI();
    refreshAll();
  </script>
</body>
</html>
"##;
