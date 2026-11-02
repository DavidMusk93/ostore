//! Modern Single-Page Application (SPA) Web UI for ostore.
//!
//! Crafted to strictly adhere to Owner UI Taste & Operational Excellence:
//! - Apple-design restraint: Light theme (#F5F5F7, #1D1D1F, #0071E3).
//! - Dedicated Mobile Viewport & Touch Optimization (anti-distortion button CSS).
//! - Clean, popup-free login transition: zero unwanted OTP/token modals post-login.
//! - Directory Dual-Metrics: Displays both Total Aggregated Size and Total Item Count.
//! - Complete Folder Capabilities: Download directory as tar, rename/move, inspect, sync, upload folder tree.
//! - Server-side incremental Lazy Load (limit/offset stream pagination) - zero open lag.
//! - Multi-node visual replica badges per row (`● cc` `● cc2` `● rn` / `○ cc2` `○ rn`).
//! - Intelligent multi-encoding text decoding (UTF-8, GB18030/GBK, UTF-16, binary detection).
//! - Complete Metrics performance diagnostics (Server-Timing header + client round-trip).

use axum::response::Html;

pub async fn handler() -> Html<&'static str> {
    Html(UI_HTML)
}

const UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover">
  <title>ostore — Distributed Object Storage</title>
  <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='%230071E3'><path d='M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5'/></svg>">
  <style>
    :root {
      --bg: #F5F5F7;
      --card-bg: rgba(255, 255, 255, 0.94);
      --card-border: rgba(0, 0, 0, 0.08);
      --card-hover: rgba(0, 0, 0, 0.025);
      --ink: #1D1D1F;
      --ink-secondary: #86868B;
      --ink-tertiary: #AEAEB2;
      --accent: #0071E3;
      --accent-hover: #0077ED;
      --accent-soft: rgba(0, 113, 227, 0.08);
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
      --shadow-md: 0 4px 20px rgba(0, 0, 0, 0.06), 0 1px 2px rgba(0, 0, 0, 0.04);
      --shadow-lg: 0 12px 36px rgba(0, 0, 0, 0.08), 0 2px 4px rgba(0, 0, 0, 0.04);
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
      -webkit-text-size-adjust: 100%;
    }

    /* Universal Anti-Distortion Button Reset */
    button, .btn, .login-btn {
      -webkit-appearance: none;
      -moz-appearance: none;
      appearance: none;
      touch-action: manipulation;
      user-select: none;
      -webkit-user-select: none;
      font-family: inherit;
    }

    /* Login Screen Layout */
    .login-wrapper {
      position: fixed;
      inset: 0;
      background: var(--bg);
      z-index: 2000;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 16px;
    }

    .login-card {
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-lg);
      padding: 36px 28px;
      width: 100%;
      max-width: 400px;
      box-shadow: var(--shadow-lg);
      text-align: center;
      display: flex;
      flex-direction: column;
      align-items: center;
    }

    .login-logo {
      width: 48px;
      height: 48px;
      background: #FAFAFC;
      border: 1px solid var(--card-border);
      border-radius: 12px;
      display: flex;
      align-items: center;
      justify-content: center;
      margin-bottom: 18px;
      box-shadow: var(--shadow-sm);
    }

    .login-logo svg { width: 24px; height: 24px; fill: var(--accent); }
    .login-card h2 { font-size: 1.3rem; font-weight: 600; letter-spacing: -0.02em; margin-bottom: 8px; }
    .login-card p { font-size: 0.82rem; color: var(--ink-secondary); line-height: 1.45; margin-bottom: 22px; }

    /* 6-Digit OTP Box (Adaptive Flex, zero overflow) */
    .otp-input-group {
      display: flex;
      gap: 6px;
      justify-content: center;
      margin-bottom: 22px;
      width: 100%;
      max-width: 330px;
    }

    .otp-digit {
      flex: 1 1 0;
      min-width: 0;
      height: 52px;
      font-family: var(--font-mono);
      font-size: 1.35rem;
      font-weight: 600;
      text-align: center;
      background: #F9F9FB;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      color: var(--ink);
      outline: none;
      transition: all 0.15s;
      -webkit-appearance: none;
    }

    .otp-digit:focus {
      border-color: var(--accent);
      background: #FFFFFF;
      box-shadow: 0 0 0 3px var(--accent-soft);
    }

    .login-btn {
      width: 100%;
      height: 46px;
      line-height: 46px;
      padding: 0 16px;
      font-size: 0.92rem;
      font-weight: 500;
      border-radius: var(--radius-sm);
      cursor: pointer;
      background: var(--accent);
      border: 1px solid var(--accent);
      color: #FFFFFF;
      transition: all 0.15s;
      box-shadow: 0 2px 8px rgba(0, 113, 227, 0.25);
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .login-btn:hover { background: var(--accent-hover); }

    .login-footer-links {
      margin-top: 20px;
      display: flex;
      flex-direction: column;
      gap: 8px;
      font-size: 0.78rem;
    }

    .login-link {
      color: var(--accent);
      text-decoration: none;
      cursor: pointer;
    }

    .login-link:hover { text-decoration: underline; }

    /* Main Console Container */
    .container {
      max-width: 1260px;
      margin: 0 auto;
      padding: 28px 20px;
      width: 100%;
      flex: 1;
    }

    @media (max-width: 640px) {
      .container { padding: 16px 12px; }
      .login-card { padding: 28px 18px; }
      .otp-digit { height: 46px; font-size: 1.2rem; }
    }

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

    header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-bottom: 20px;
      margin-bottom: 24px;
      border-bottom: 1px solid var(--card-border);
      flex-wrap: wrap;
      gap: 12px;
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
      gap: 8px;
      flex-wrap: wrap;
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
      height: 34px;
      box-sizing: border-box;
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
      justify-content: center;
      gap: 6px;
      height: 34px;
      padding: 0 14px;
      border-radius: var(--radius-sm);
      font-size: 0.82rem;
      font-weight: 500;
      cursor: pointer;
      border: 1px solid var(--card-border);
      background: #FFFFFF;
      color: var(--ink);
      transition: all 0.15s ease;
      box-shadow: var(--shadow-sm);
      white-space: nowrap;
      box-sizing: border-box;
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

    .btn-primary:hover { background: var(--accent-hover); }

    .panel {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--radius-md);
      backdrop-filter: blur(20px);
      box-shadow: var(--shadow-sm);
      margin-bottom: 24px;
      overflow: hidden;
    }

    .panel-header {
      padding: 14px 18px;
      border-bottom: 1px solid var(--card-border);
      display: flex;
      justify-content: space-between;
      align-items: center;
      flex-wrap: wrap;
      gap: 10px;
    }

    .panel-title {
      font-size: 0.95rem;
      font-weight: 600;
      letter-spacing: -0.01em;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .topology-wrap {
      padding: 16px;
      display: grid;
      grid-template-columns: 1fr 340px;
      gap: 16px;
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
      overflow: hidden;
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

    .spectrum-panel {
      padding: 14px 18px;
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
      gap: 14px;
      font-size: 0.74rem;
      color: var(--ink-secondary);
    }

    .spectrum-legend-item {
      display: flex;
      align-items: center;
      gap: 5px;
    }

    .legend-dot { width: 7px; height: 7px; border-radius: 2px; }

    .explorer-bar {
      padding: 12px 18px;
      border-bottom: 1px solid var(--card-border);
      display: flex;
      justify-content: space-between;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
    }

    .breadcrumbs-wrap {
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
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

    .crumb-link:hover { color: var(--ink); background: rgba(0, 0, 0, 0.04); }
    .crumb-sep { color: var(--ink-tertiary); font-size: 0.75rem; }

    .folder-stat-badge {
      font-family: var(--font-mono);
      font-size: 0.72rem;
      padding: 3px 8px;
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: 6px;
      color: var(--ink-secondary);
      white-space: nowrap;
    }

    .search-input {
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      height: 34px;
      padding: 0 12px;
      color: var(--ink);
      font-size: 16px; /* 16px prevents iOS zoom-on-focus */
      width: 170px;
      outline: none;
      transition: all 0.2s;
      box-sizing: border-box;
    }

    @media (min-width: 640px) {
      .search-input { font-size: 0.82rem; }
      .search-input:focus { width: 220px; }
    }

    .table-viewport {
      max-height: 580px;
      overflow-y: auto;
      overflow-x: auto;
      position: relative;
      -webkit-overflow-scrolling: touch;
    }

    .file-table {
      width: 100%;
      min-width: 640px;
      border-collapse: collapse;
      text-align: left;
      font-size: 0.84rem;
    }

    .file-table th {
      position: sticky;
      top: 0;
      background: #FAFAFC;
      padding: 10px 16px;
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

    .file-row:hover { background: var(--card-hover); }

    .file-row td {
      padding: 10px 16px;
      vertical-align: middle;
    }

    .name-cell {
      display: flex;
      align-items: center;
      gap: 10px;
      cursor: pointer;
      font-weight: 500;
    }

    .name-cell svg { width: 16px; height: 16px; flex-shrink: 0; }
    .meta-mono { font-family: var(--font-mono); font-size: 0.76rem; color: var(--ink-secondary); }

    .dir-size-cell {
      display: flex;
      flex-direction: column;
      gap: 2px;
      cursor: pointer;
    }

    .dir-size-val { font-weight: 600; color: var(--ink); }
    .dir-items-val { font-size: 0.7rem; color: var(--ink-secondary); }

    .node-badges-cluster {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 3px 8px;
      border-radius: 980px;
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      box-shadow: 0 1px 2px rgba(0,0,0,0.03);
      cursor: pointer;
      font-family: var(--font-mono);
      font-size: 0.72rem;
      transition: all 0.15s;
      white-space: nowrap;
    }

    .node-badges-cluster:hover {
      background: #F9F9FB;
      border-color: rgba(0,0,0,0.15);
      transform: translateY(-0.5px);
    }

    .node-dot-item {
      display: inline-flex;
      align-items: center;
      gap: 3px;
      color: var(--ink-secondary);
    }

    .node-dot-item.present { color: var(--ink); font-weight: 600; }

    .mini-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
      display: inline-block;
    }

    .mini-dot.main { background: var(--accent); }
    .mini-dot.present { background: var(--success); }
    .mini-dot.missing { background: #D2D2D7; border: 1px solid #AEAEB2; box-sizing: border-box; }

    .ratio-badge {
      font-size: 0.68rem;
      padding: 1px 5px;
      border-radius: 4px;
      margin-left: 2px;
    }

    .ratio-badge.full { background: var(--success-soft); color: #248A3D; }
    .ratio-badge.partial { background: var(--warning-soft); color: #B25E00; }

    .row-actions {
      display: flex;
      align-items: center;
      gap: 6px;
      justify-content: flex-end;
    }

    @media (min-width: 768px) {
      .row-actions {
        opacity: 0;
        transition: opacity 0.15s ease;
      }
      .file-row:hover .row-actions { opacity: 1; }
    }

    .icon-btn {
      background: transparent;
      border: none;
      padding: 6px;
      border-radius: 4px;
      cursor: pointer;
      color: var(--ink-secondary);
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.12s;
    }

    .icon-btn:hover { color: var(--ink); background: rgba(0, 0, 0, 0.06); }
    .icon-btn.btn-delete:hover { color: var(--danger); background: var(--danger-soft); }

    .backdrop {
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.35);
      backdrop-filter: blur(8px);
      z-index: 500;
      display: none;
      align-items: center;
      justify-content: center;
      padding: 16px;
    }

    .dialog-box {
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-md);
      padding: 24px;
      width: 100%;
      max-width: 480px;
      box-shadow: var(--shadow-md);
    }

    .dialog-box h3 { font-size: 1.05rem; font-weight: 600; margin-bottom: 12px; }

    .file-topo-box { max-width: 640px; }
    .file-topo-flow { display: flex; flex-direction: column; gap: 10px; margin: 16px 0; }
    .file-node-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 10px 14px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--card-border);
      background: #F9F9FB;
    }

    .metrics-bar {
      font-family: var(--font-mono);
      font-size: 0.72rem;
      color: var(--ink-secondary);
      background: #F5F5F7;
      padding: 6px 10px;
      border-radius: 6px;
      margin-top: 10px;
      display: flex;
      justify-content: space-between;
      align-items: center;
    }

    .media-box {
      max-width: 90vw;
      max-height: 85vh;
      padding: 18px;
      display: flex;
      flex-direction: column;
      background: #FFFFFF;
    }

    .media-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-bottom: 10px;
      margin-bottom: 10px;
      border-bottom: 1px solid var(--card-border);
    }

    .media-container {
      max-width: 85vw;
      max-height: 68vh;
      overflow: auto;
      display: flex;
      justify-content: center;
      align-items: center;
    }

    .media-container pre {
      font-family: var(--font-mono);
      font-size: 0.8rem;
      background: #FAFAFC;
      border: 1px solid var(--card-border);
      padding: 14px;
      border-radius: 8px;
      width: 100%;
      max-height: 65vh;
      overflow: auto;
      color: #1D1D1F;
      white-space: pre-wrap;
      line-height: 1.5;
    }

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
  <!-- Authenticator OTP Login View (Apple-Design Light Theme) -->
  <div class="login-wrapper" id="loginView" style="display: none;">
    <div class="login-card">
      <div class="login-logo">
        <svg viewBox="0 0 24 24"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>
      </div>
      <h2>Sign in to ostore</h2>
      <p>Enter the 6-digit verification code from your Authenticator app (Apple Passwords, Google Authenticator, etc.)</p>

      <div class="otp-input-group">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*" autofocus>
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*">
      </div>

      <button class="login-btn" id="loginBtn" onclick="submitOtpLogin()">Sign In</button>

      <div class="login-footer-links">
        <a class="login-link" onclick="openTotpSetupModal()">Set up Authenticator (QR & Secret)</a>
        <a class="login-link" style="color:var(--ink-secondary);" onclick="openTokenModal()">Use developer token</a>
      </div>
    </div>
  </div>

  <!-- Main Console -->
  <div class="container" id="mainConsole" style="display: none;">
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
        <button class="btn" onclick="signOut()" title="Sign out current session">
          <span>Sign Out</span>
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
        <div class="topo-canvas-container">
          <svg id="topoCanvas" width="100%" height="220" viewBox="0 0 540 220" style="overflow: visible;"></svg>
        </div>
        <div class="topo-cards-grid" id="topoCardsList"></div>
      </div>

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
        <div class="breadcrumbs-wrap">
          <div class="breadcrumbs" id="breadcrumbs">
            <span class="crumb-link" onclick="navigate('')">root</span>
          </div>
          <span class="folder-stat-badge" id="folderStatBadge">0 items · 0 B</span>
        </div>
        <div style="display: flex; gap: 8px; align-items: center; flex-wrap: wrap;">
          <input type="text" class="search-input" id="searchBox" placeholder="Filter objects..." oninput="onFilter()">
          <button class="btn" onclick="openMkdir()">New Folder</button>
          <button class="btn" onclick="triggerFolderUploadInput()" title="Upload entire folder tree">Upload Folder</button>
          <button class="btn btn-primary" onclick="triggerFileInput()">Upload File</button>
          <input type="file" id="fileInput" multiple style="display: none;" onchange="handleFileSelect(event)">
          <input type="file" id="folderInput" webkitdirectory directory multiple style="display: none;" onchange="handleFolderSelect(event)">
        </div>
      </div>

      <div class="table-viewport" id="tableViewport">
        <table class="file-table">
          <thead>
            <tr>
              <th style="width: 42%;">Name</th>
              <th style="width: 16%;">Size & Total Items</th>
              <th style="width: 16%;">Modified</th>
              <th style="width: 14%;">Replication Map</th>
              <th style="width: 12%; text-align: right;">Folder Actions</th>
            </tr>
          </thead>
          <tbody id="fileTableBody"></tbody>
        </table>
        <div id="scrollAnchor" style="height: 24px; text-align: center; color: var(--ink-secondary); font-size: 0.75rem; padding: 6px;"></div>
      </div>
    </section>
  </div>

  <!-- Authenticator Setup Modal -->
  <div class="backdrop" id="totpSetupModal">
    <div class="dialog-box">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <h3>Set Up Authenticator</h3>
        <button class="icon-btn" onclick="closeTotpSetupModal()">✕</button>
      </div>
      <p style="font-size: 0.82rem; color: var(--ink-secondary); margin-bottom: 14px;">
        Add ostore to Apple Passwords, Google Authenticator, or 1Password using this Secret:
      </p>

      <div style="background: #F9F9FB; border: 1px solid var(--card-border); padding: 14px; border-radius: var(--radius-sm); margin-bottom: 14px;">
        <div style="font-size: 0.75rem; color: var(--ink-secondary); margin-bottom: 4px;">BASE32 SECRET KEY</div>
        <div style="font-family: var(--font-mono); font-size: 1.15rem; font-weight: 600; letter-spacing: 0.08em; color: var(--accent); word-break: break-all;" id="totpSecretText">
          LOADING...
        </div>
      </div>

      <div style="font-size: 0.75rem; color: var(--ink-secondary); font-family: var(--font-mono); word-break: break-all; margin-bottom: 16px;">
        URI: <span id="totpUriText">--</span>
      </div>

      <div style="display: flex; justify-content: flex-end; gap: 8px;">
        <button class="btn btn-primary" onclick="copyTotpSecret()">Copy Secret</button>
      </div>
    </div>
  </div>

  <!-- Token Modal (Fallback) -->
  <div class="backdrop" id="tokenModal">
    <div class="dialog-box">
      <h3>Developer Token</h3>
      <p style="font-size: 0.82rem; color: var(--ink-secondary); margin-bottom: 12px;">Manual OSTORE_TOKEN override:</p>
      <input type="password" id="tokenInput" class="search-input" style="width: 100%;" placeholder="OSTORE_TOKEN">
      <div style="display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px;">
        <button class="btn" onclick="closeTokenModal()">Cancel</button>
        <button class="btn btn-primary" onclick="saveToken()">Save</button>
      </div>
    </div>
  </div>

  <!-- Folder Details Modal -->
  <div class="backdrop" id="folderDetailModal">
    <div class="dialog-box">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <h3 id="folderDetailTitle">Folder Details</h3>
        <button class="icon-btn" onclick="closeFolderDetailModal()">✕</button>
      </div>
      <p style="font-size: 0.78rem; color: var(--ink-secondary); font-family: var(--font-mono); margin-bottom: 16px;" id="folderDetailPath"></p>

      <div style="display: flex; flex-direction: column; gap: 10px; background: #F9F9FB; padding: 14px; border-radius: var(--radius-sm); border: 1px solid var(--card-border); font-family: var(--font-mono); font-size: 0.8rem;">
        <div style="display: flex; justify-content: space-between;">
          <span style="color: var(--ink-secondary);">Total Size:</span>
          <strong id="folderDetailSize" style="color: var(--ink);">--</strong>
        </div>
        <div style="display: flex; justify-content: space-between;">
          <span style="color: var(--ink-secondary);">Total Objects:</span>
          <strong id="folderDetailCount" style="color: var(--ink);">--</strong>
        </div>
        <div style="display: flex; justify-content: space-between;">
          <span style="color: var(--ink-secondary);">Modified:</span>
          <span id="folderDetailMtime">--</span>
        </div>
      </div>

      <div style="display: flex; justify-content: space-between; gap: 8px; margin-top: 20px;">
        <a class="btn" id="folderDownloadTarBtn" href="#" download>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
          Download Tar
        </a>
        <div style="display: flex; gap: 8px;">
          <button class="btn" onclick="renameFolderPrompt()">Rename</button>
          <button class="btn btn-primary" onclick="syncFolderReplicas()">Sync Replicas</button>
        </div>
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

      <div class="metrics-bar" id="fileTopoMetrics">
        <span id="fileTopoLatency">Probe Latency: --</span>
        <span id="fileTopoMethod">Tiered Probe</span>
      </div>

      <div style="display: flex; justify-content: space-between; align-items: center; margin-top: 16px;">
        <span id="fileTopoSummary" style="font-size: 0.78rem; color: var(--ink-secondary); font-family: var(--font-mono);"></span>
        <div style="display: flex; gap: 8px;">
          <button class="btn" id="btnDeepCheck" onclick="inspectFileReplica(inspectingFilePath, true)">Deep Hash Check</button>
          <button class="btn btn-primary" id="btnSyncSingleFile" onclick="syncCurrentFile()">Sync To Followers</button>
        </div>
      </div>
    </div>
  </div>

  <!-- Media Preview Modal -->
  <div class="backdrop" id="mediaModal">
    <div class="dialog-box media-box">
      <div class="media-header">
        <div>
          <span id="mediaTitle" style="font-weight: 600; font-size: 0.9rem;"></span>
          <span id="mediaEncodingTag" style="font-family: var(--font-mono); font-size: 0.7rem; padding: 2px 6px; border-radius: 4px; background: rgba(0,0,0,0.05); margin-left: 8px;"></span>
        </div>
        <button class="icon-btn" onclick="closeMediaModal()">✕</button>
      </div>
      <div class="media-container" id="mediaContainer"></div>
      <div class="metrics-bar" id="mediaMetricsBar">
        <span id="mediaServerTiming">Loading metrics...</span>
        <span id="mediaTransferSpeed">--</span>
      </div>
    </div>
  </div>

  <div class="toast" id="toast">
    <span id="toastMsg">Action completed</span>
  </div>

  <script>
    // State Architecture
    let currentPath = '';
    let currentEntries = [];
    let currentTotalCount = 0;
    let currentTotalBytes = 0;
    let currentOffset = 0;
    let hasMore = false;
    let isLoading = false;
    const PAGE_LIMIT = 50;

    let inspectingFilePath = '';
    let inspectingFolderPath = '';
    let totpSecretRaw = '';
    const replicaMapCache = new Map();

    let sessionToken = localStorage.getItem('ostore_sess') || localStorage.getItem('ostore_token') || '';
    let eventSource = null;

    function getBasePath() {
      return window.location.pathname.startsWith('/ostore') ? '/ostore' : '';
    }

    // URL token auto-capture
    const urlParams = new URLSearchParams(window.location.search);
    if (urlParams.has('token')) {
      sessionToken = urlParams.get('token');
      localStorage.setItem('ostore_sess', sessionToken);
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
      if (sessionToken) opts.headers['Authorization'] = `Bearer ${sessionToken}`;
      return fetch(url, opts);
    }

    // Authenticator OTP UI & Flow
    const otpInputs = document.querySelectorAll('.otp-digit');
    otpInputs.forEach((input, index) => {
      input.addEventListener('input', (e) => {
        const val = e.target.value;
        if (val.length === 1 && index < otpInputs.length - 1) {
          otpInputs[index + 1].focus();
        }
        checkAutoSubmit();
      });

      input.addEventListener('keydown', (e) => {
        if (e.key === 'Backspace' && !e.target.value && index > 0) {
          otpInputs[index - 1].focus();
        } else if (e.key === 'Enter') {
          submitOtpLogin();
        }
      });

      input.addEventListener('paste', (e) => {
        e.preventDefault();
        const text = (e.clipboardData || window.clipboardData).getData('text').trim();
        if (/^\d{6}$/.test(text)) {
          text.split('').forEach((digit, i) => {
            if (otpInputs[i]) otpInputs[i].value = digit;
          });
          otpInputs[5].focus();
          submitOtpLogin();
        }
      });
    });

    function checkAutoSubmit() {
      const code = Array.from(otpInputs).map(i => i.value).join('');
      if (code.length === 6) {
        submitOtpLogin();
      }
    }

    async function submitOtpLogin() {
      const code = Array.from(otpInputs).map(i => i.value).join('');
      if (code.length !== 6) {
        alert('Please enter a valid 6-digit Authenticator code');
        return;
      }

      const btn = document.getElementById('loginBtn');
      btn.textContent = 'Verifying...';
      btn.disabled = true;

      try {
        const res = await fetch(`${getBasePath()}/api/auth/login`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ code })
        });

        if (res.ok) {
          const data = await res.json();
          sessionToken = data.token;
          localStorage.setItem('ostore_sess', sessionToken);
          // Force close any setup/token modal on login
          closeTotpSetupModal();
          closeTokenModal();
          showToast('Authenticator verified');
          showConsole();
        } else {
          alert('Invalid verification code. Please check your Authenticator app.');
          otpInputs.forEach(i => i.value = '');
          otpInputs[0].focus();
        }
      } catch (e) {
        alert('Authentication error: ' + e);
      } finally {
        btn.textContent = 'Sign In';
        btn.disabled = false;
      }
    }

    async function openTotpSetupModal() {
      try {
        const res = await fetch(`${getBasePath()}/api/auth/setup`);
        if (res.ok) {
          const data = await res.json();
          totpSecretRaw = data.secret;
          document.getElementById('totpSecretText').textContent = data.secret;
          document.getElementById('totpUriText').textContent = data.otpauth_uri;
          document.getElementById('totpSetupModal').style.display = 'flex';
        }
      } catch (e) {
        alert('Failed to load setup info: ' + e);
      }
    }

    function closeTotpSetupModal() {
      document.getElementById('totpSetupModal').style.display = 'none';
    }

    function copyTotpSecret() {
      navigator.clipboard.writeText(totpSecretRaw).then(() => {
        showToast('Secret copied to clipboard');
      });
    }

    async function signOut() {
      await api('/api/auth/logout', { method: 'POST' });
      sessionToken = '';
      localStorage.removeItem('ostore_sess');
      localStorage.removeItem('ostore_token');
      if (eventSource) eventSource.close();
      showLogin();
    }

    function showLogin() {
      // Close all modals cleanly
      closeAllModals();
      document.getElementById('loginView').style.display = 'flex';
      document.getElementById('mainConsole').style.display = 'none';
      otpInputs.forEach(i => i.value = '');
      otpInputs[0].focus();
    }

    function showConsole() {
      // Close all modals cleanly: NEVER leave setup or token modal open post-login
      closeAllModals();
      document.getElementById('loginView').style.display = 'none';
      document.getElementById('mainConsole').style.display = 'block';
      initSSE();
      loadTopology();
      loadDirectory('', false);
    }

    function closeAllModals() {
      document.getElementById('totpSetupModal').style.display = 'none';
      document.getElementById('tokenModal').style.display = 'none';
      document.getElementById('folderDetailModal').style.display = 'none';
      document.getElementById('fileTopoModal').style.display = 'none';
      document.getElementById('mediaModal').style.display = 'none';
    }

    // Reactive SSE: Incremental updates without full-page redraws
    function initSSE() {
      if (eventSource) eventSource.close();
      const sseUrl = `${getBasePath()}/api/events${sessionToken ? `?token=${encodeURIComponent(sessionToken)}` : ''}`;
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
      eventSource.addEventListener('stream.reset', () => loadDirectory(currentPath, false));
    }

    function onObjectEvent(data, kind) {
      const p = data.path;
      replicaMapCache.delete(p);
      const cur = currentPath ? currentPath + '/' : '';
      if (!p.startsWith(cur)) return;
      const sub = p.substring(cur.length);
      if (sub.includes('/')) return;

      const idx = currentEntries.findIndex(e => e.name === sub);
      if (kind === 'deleted') {
        if (idx !== -1) {
          currentEntries.splice(idx, 1);
          renderTable(currentEntries, false);
        }
      } else if (kind === 'created' || kind === 'updated') {
        const item = {
          name: sub,
          path: p,
          type: data.type || 'file',
          size: data.size || 0,
          item_count: data.type === 'dir' ? 0 : null,
          mtime: data.ts || new Date().toISOString(),
          sha256: null
        };
        if (idx !== -1) currentEntries[idx] = item;
        else currentEntries.unshift(item);
        renderTable(currentEntries, false);
      }
    }

    // Load Topology: Main & Follower Graph
    async function loadTopology() {
      try {
        const res = await api('/api/cluster/topology');
        if (!res.ok) {
          if (res.status === 401) showLogin();
          return;
        }
        const topo = await res.json();

        renderTopologySVG(topo);

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
        <path d="M270 65 Q210 115 150 160" fill="none" stroke="#D2D2D7" stroke-width="2" stroke-dasharray="4 4"/>
        <path d="M270 65 Q330 115 390 160" fill="none" stroke="#D2D2D7" stroke-width="2" stroke-dasharray="4 4"/>

        <!-- Main Node (cc) -->
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

    // Object Explorer: True Lazy Load
    async function loadDirectory(path = currentPath, append = false) {
      if (isLoading) return;
      isLoading = true;

      if (!append) {
        currentPath = path;
        currentOffset = 0;
        currentEntries = [];
        updateBreadcrumbs();
        document.getElementById('fileTableBody').innerHTML = `
          <tr><td colspan="5" style="text-align:center; padding: 24px; color: var(--ink-secondary); font-family:var(--font-mono);">
            ⚡ Reading directory metadata...
          </td></tr>
        `;
      }

      try {
        let query = `?limit=${PAGE_LIMIT}&offset=${currentOffset}`;
        if (currentPath) query += `&path=${encodeURIComponent(currentPath)}`;

        const res = await api(`/api/list${query}`);
        if (!res.ok) {
          if (res.status === 401) {
            showLogin();
            return;
          }
          throw new Error(`HTTP ${res.status}`);
        }
        const data = await res.json();

        currentTotalCount = data.total_count || 0;
        currentTotalBytes = data.total_bytes || 0;
        hasMore = data.has_more || false;

        document.getElementById('folderStatBadge').textContent = 
          `${currentTotalCount.toLocaleString()} items · Total ${formatBytes(currentTotalBytes)}`;

        const newEntries = data.entries || [];
        if (append) {
          currentEntries = currentEntries.concat(newEntries);
        } else {
          currentEntries = newEntries;
        }

        currentOffset = currentEntries.length;
        renderTable(currentEntries, append);

        document.getElementById('scrollAnchor').textContent = hasMore 
          ? `Scroll down for more (${currentEntries.length}/${currentTotalCount})...` 
          : `Showing all ${currentTotalCount.toLocaleString()} items`;

      } catch (e) {
        console.error('List error:', e);
      } finally {
        isLoading = false;
      }
    }

    function renderTable(entries, append) {
      const tbody = document.getElementById('fileTableBody');
      const q = document.getElementById('searchBox').value.toLowerCase();
      const filtered = q ? entries.filter(e => e.name.toLowerCase().includes(q)) : entries;

      if (!append) {
        tbody.innerHTML = '';
      }

      if (filtered.length === 0 && !append) {
        tbody.innerHTML = `<tr><td colspan="5" style="text-align:center; padding: 32px; color: var(--ink-secondary);">Folder is empty</td></tr>`;
        return;
      }

      const rows = (append ? filtered.slice(filtered.length - PAGE_LIMIT) : filtered).map(item => {
        const isDir = item.type === 'dir';
        const dlUrl = isDir ? `${getBasePath()}/api/archive?path=${item.path}` : `${getBasePath()}/api/files/${item.path}`;

        let nodeMapHtml = '-';
        if (!isDir) {
          const cached = replicaMapCache.get(item.path);
          const hasCc2 = cached ? cached.cc2 : null;
          const hasRn = cached ? cached.rn : null;

          const cc2DotClass = hasCc2 === true ? 'present' : 'missing';
          const rnDotClass = hasRn === true ? 'present' : 'missing';
          const count = (1 + (hasCc2 === true ? 1 : 0) + (hasRn === true ? 1 : 0));
          const ratioClass = count === 3 ? 'full' : 'partial';

          nodeMapHtml = `
            <div class="node-badges-cluster" id="badge-${escapeId(item.path)}" onclick="inspectFileReplica('${item.path}')" title="Click to inspect and sync replicas across nodes">
              <span class="node-dot-item present"><span class="mini-dot main"></span>cc</span>
              <span class="node-dot-item ${hasCc2 === true ? 'present' : ''}"><span class="mini-dot ${cc2DotClass}"></span>cc2</span>
              <span class="node-dot-item ${hasRn === true ? 'present' : ''}"><span class="mini-dot ${rnDotClass}"></span>rn</span>
              <span class="ratio-badge ${ratioClass}">${count}/3</span>
            </div>
          `;
        }

        let sizeDisplayHtml = '';
        if (isDir) {
          const itemCount = item.item_count !== undefined && item.item_count !== null ? `${item.item_count.toLocaleString()} items` : 'Directory';
          sizeDisplayHtml = `
            <div class="dir-size-cell" onclick="openFolderDetail('${item.path}', '${item.name}', ${item.size}, ${item.item_count || 0}, '${item.mtime}')" title="Click to inspect folder properties">
              <span class="dir-size-val">${formatBytes(item.size)}</span>
              <span class="dir-items-val">${itemCount}</span>
            </div>
          `;
        } else {
          sizeDisplayHtml = `<span class="meta-mono">${formatBytes(item.size)}</span>`;
        }

        let actionsHtml = '';
        if (isDir) {
          actionsHtml = `
            <div class="row-actions">
              <a class="icon-btn" href="${dlUrl}" download title="Download folder as tar archive">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
              </a>
              <button class="icon-btn" title="Folder properties and sync" onclick="openFolderDetail('${item.path}', '${item.name}', ${item.size}, ${item.item_count || 0}, '${item.mtime}')">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
              </button>
              <button class="icon-btn" title="Rename folder" onclick="renamePrompt('${item.path}')">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
              </button>
              <button class="icon-btn btn-delete" title="Delete folder" onclick="deleteItem('${item.path}', true)">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
              </button>
            </div>
          `;
        } else {
          actionsHtml = `
            <div class="row-actions">
              <a class="icon-btn" href="${dlUrl}" download title="Download">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
              </a>
              <button class="icon-btn" title="Copy Direct URL" onclick="copyURL('${dlUrl}')">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
              </button>
              <button class="icon-btn" title="Copy SHA-256" onclick="copySHA('${item.sha256}')">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
              </button>
              <button class="icon-btn btn-delete" title="Delete" onclick="deleteItem('${item.path}', false)">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
              </button>
            </div>
          `;
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
            <td>${sizeDisplayHtml}</td>
            <td class="meta-mono">${formatDate(item.mtime)}</td>
            <td>${nodeMapHtml}</td>
            <td>${actionsHtml}</td>
          </tr>
        `;
      }).join('');

      tbody.insertAdjacentHTML('beforeend', rows);
    }

    function onFilter() {
      renderTable(currentEntries, false);
    }

    function escapeId(str) {
      return btoa(unescape(encodeURIComponent(str))).replace(/[/+=]/g, '_');
    }

    const scrollObserver = new IntersectionObserver(entries => {
      if (entries[0].isIntersecting && hasMore && !isLoading) {
        loadDirectory(currentPath, true);
      }
    }, { root: document.getElementById('tableViewport'), threshold: 0.1 });

    scrollObserver.observe(document.getElementById('scrollAnchor'));

    function navigate(p) {
      loadDirectory(p, false);
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

    function openFolderDetail(path, name, size, count, mtime) {
      inspectingFolderPath = path;
      document.getElementById('folderDetailTitle').textContent = `Folder: ${name}`;
      document.getElementById('folderDetailPath').textContent = path;
      document.getElementById('folderDetailSize').textContent = formatBytes(size);
      document.getElementById('folderDetailCount').textContent = `${(count || 0).toLocaleString()} objects inside`;
      document.getElementById('folderDetailMtime').textContent = formatDate(mtime);
      document.getElementById('folderDownloadTarBtn').href = `${getBasePath()}/api/archive?path=${encodeURIComponent(path)}`;
      document.getElementById('folderDetailModal').style.display = 'flex';
    }

    function closeFolderDetailModal() {
      document.getElementById('folderDetailModal').style.display = 'none';
    }

    async function renamePrompt(oldPath) {
      const curName = oldPath.split('/').pop();
      const newName = prompt('New name:', curName);
      if (!newName || newName === curName) return;

      const parent = oldPath.includes('/') ? oldPath.substring(0, oldPath.lastIndexOf('/')) : '';
      const newPath = parent ? `${parent}/${newName}` : newName;

      const res = await api('/api/move', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from: oldPath, to: newPath })
      });
      if (res.ok) {
        showToast('Renamed successfully');
        loadDirectory(currentPath, false);
      } else {
        alert('Rename failed');
      }
    }

    async function renameFolderPrompt() {
      if (!inspectingFolderPath) return;
      closeFolderDetailModal();
      renamePrompt(inspectingFolderPath);
    }

    async function syncFolderReplicas() {
      if (!inspectingFolderPath) return;
      showToast(`Syncing folder "${inspectingFolderPath}" to all followers...`);
      const res = await api(`/api/cluster/sync?prefix=${encodeURIComponent(inspectingFolderPath)}`, { method: 'POST' });
      if (res.ok) {
        const r = await res.json();
        showToast(`Folder synced in ${r.duration_ms}ms`);
        closeFolderDetailModal();
        loadDirectory(currentPath, false);
      }
    }

    async function inspectFileReplica(filePath, deepVerify = false) {
      inspectingFilePath = filePath;
      document.getElementById('fileTopoPath').textContent = filePath;
      document.getElementById('fileTopoTitle').textContent = `Replica Topology: ${filePath.split('/').pop()}`;
      document.getElementById('fileTopoFlow').innerHTML = '<div style="text-align:center; padding: 20px; color: var(--ink-secondary);">Probing cluster nodes...</div>';
      document.getElementById('fileTopoMetrics').innerHTML = '<span>Probing cluster network...</span><span>--</span>';
      document.getElementById('fileTopoModal').style.display = 'flex';

      try {
        const query = deepVerify ? `?path=${encodeURIComponent(filePath)}&verify_hash=1` : `?path=${encodeURIComponent(filePath)}`;
        const res = await api(`/api/cluster/file-status${query}`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();

        const mainNode = data.main;
        const followers = data.followers || [];
        const metrics = data.metrics || {};

        let hasCc2 = false;
        let hasRn = false;

        let html = `
          <div class="file-node-row">
            <div>
              <div style="font-weight:600; font-size: 0.84rem; display: flex; align-items: center; gap: 6px;">
                <span class="status-dot ${mainNode.exists ? '' : 'disconnected'}"></span>
                <span>${mainNode.name}</span>
                <span class="topo-card-badge">Main Node (Control Plane)</span>
              </div>
              <div style="font-size: 0.72rem; font-family: var(--font-mono); color: var(--ink-secondary); margin-top: 4px;">
                ${mainNode.sha256 ? `SHA-256: ${mainNode.sha256.substring(0, 16)}...` : 'Status: Present on Main Local Disk'}
              </div>
            </div>
            <div style="text-align: right; font-family: var(--font-mono); font-size: 0.78rem;">
              <div>${formatBytes(mainNode.size)}</div>
              <span class="ratio-badge full" style="font-size:0.7rem;">Main Present</span>
            </div>
          </div>
        `;

        for (const f of followers) {
          const isMatched = f.exists && f.matches;
          if (f.id === 'cc2' && isMatched) hasCc2 = true;
          if (f.id === 'racknerd' && isMatched) hasRn = true;

          html += `
            <div class="file-node-row">
              <div>
                <div style="font-weight:600; font-size: 0.84rem; display: flex; align-items: center; gap: 6px;">
                  <span class="status-dot ${isMatched ? '' : 'disconnected'}"></span>
                  <span>${f.name}</span>
                  <span class="topo-card-badge">Follower · ${f.rtt_ms !== null ? f.rtt_ms + 'ms' : 'off'}</span>
                </div>
                <div style="font-size: 0.72rem; font-family: var(--font-mono); color: var(--ink-secondary); margin-top: 4px;">
                  ${f.exists 
                    ? (f.sha256 ? `SHA-256: ${f.sha256.substring(0, 16)}...` : 'Matched Size & ETag') 
                    : '<strong style="color:var(--danger)">Missing on this node</strong>'}
                </div>
              </div>
              <div style="text-align: right; font-family: var(--font-mono); font-size: 0.78rem;">
                <div>${f.exists ? formatBytes(f.size) : '0 B'}</div>
                <span class="ratio-badge ${isMatched ? 'full' : 'partial'}" style="font-size:0.7rem;">
                  ${isMatched ? 'Replica Synced' : (f.exists ? 'Size Mismatch' : 'Not Replicated')}
                </span>
              </div>
            </div>
          `;
        }

        document.getElementById('fileTopoFlow').innerHTML = html;

        let timingBreakdown = `Total: ${metrics.total_ms}ms (Main: ${metrics.main_lookup_ms}ms`;
        for (const [k, v] of Object.entries(metrics.follower_timings || {})) {
          timingBreakdown += `, ${k}: ${v}ms`;
        }
        timingBreakdown += ')';
        document.getElementById('fileTopoMetrics').innerHTML = `
          <span>Probe Latency: <strong>${timingBreakdown}</strong></span>
          <span>Mode: <strong>${metrics.verify_hash ? 'Deep Hash' : 'Fast Tiered Probe'}</strong></span>
        `;

        replicaMapCache.set(filePath, { cc2: hasCc2, rn: hasRn });
        updateRowBadge(filePath, hasCc2, hasRn);

        document.getElementById('fileTopoSummary').textContent = 
          `Status: ${data.replica_count}/${data.expected_replicas} Nodes Hold Replicas`;

      } catch (e) {
        document.getElementById('fileTopoFlow').innerHTML = `<div style="color: var(--danger); padding: 14px;">Error querying replica: ${e}</div>`;
      }
    }

    function updateRowBadge(filePath, hasCc2, hasRn) {
      const badge = document.getElementById(`badge-${escapeId(filePath)}`);
      if (!badge) return;
      const cc2DotClass = hasCc2 === true ? 'present' : 'missing';
      const rnDotClass = hasRn === true ? 'present' : 'missing';
      const count = 1 + (hasCc2 === true ? 1 : 0) + (hasRn === true ? 1 : 0);
      const ratioClass = count === 3 ? 'full' : 'partial';

      badge.innerHTML = `
        <span class="node-dot-item present"><span class="mini-dot main"></span>cc</span>
        <span class="node-dot-item ${hasCc2 === true ? 'present' : ''}"><span class="mini-dot ${cc2DotClass}"></span>cc2</span>
        <span class="node-dot-item ${hasRn === true ? 'present' : ''}"><span class="mini-dot ${rnDotClass}"></span>rn</span>
        <span class="ratio-badge ${ratioClass}">${count}/3</span>
      `;
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

    // Intelligent Text Decoding
    function decodeTextSmart(uint8Array) {
      if (uint8Array.length >= 8) {
        const magic = String.fromCharCode(...uint8Array.slice(0, 8));
        if (magic.startsWith('XSTRMENC') || magic.startsWith('\x7fELF') || magic.startsWith('SQLite')) {
          return {
            isBinary: true,
            encoding: `Binary / Encrypted (${magic.trim()})`,
            text: renderHex(uint8Array.slice(0, 512))
          };
        }
      }

      let nullCount = 0;
      const sampleLen = Math.min(uint8Array.length, 1024);
      for (let i = 0; i < sampleLen; i++) {
        if (uint8Array[i] === 0) nullCount++;
      }
      if (nullCount > 2) {
        return {
          isBinary: true,
          encoding: 'Binary Data',
          text: renderHex(uint8Array.slice(0, 512))
        };
      }

      try {
        const text = new TextDecoder('utf-8', { fatal: true }).decode(uint8Array);
        return { isBinary: false, encoding: 'UTF-8', text };
      } catch (_) {}

      try {
        const text = new TextDecoder('gb18030', { fatal: true }).decode(uint8Array);
        return { isBinary: false, encoding: 'GB18030 / GBK', text };
      } catch (_) {}

      try {
        const text = new TextDecoder('utf-16le', { fatal: true }).decode(uint8Array);
        return { isBinary: false, encoding: 'UTF-16LE', text };
      } catch (_) {}

      return {
        isBinary: false,
        encoding: 'UTF-8 (Lossy)',
        text: new TextDecoder('utf-8').decode(uint8Array)
      };
    }

    function renderHex(bytes) {
      let out = '';
      for (let i = 0; i < bytes.length; i += 16) {
        const slice = bytes.slice(i, i + 16);
        const hex = Array.from(slice).map(b => b.toString(16).padStart(2, '0')).join(' ');
        const ascii = Array.from(slice).map(b => (b >= 32 && b <= 126 ? String.fromCharCode(b) : '.')).join('');
        out += `${i.toString(16).padStart(6, '0')}:  ${hex.padEnd(48, ' ')}  |${ascii}|\n`;
      }
      return out;
    }

    async function openMediaPreview(path) {
      const ext = path.split('.').pop().toLowerCase();
      const url = `${getBasePath()}/api/files/${path}`;
      const modal = document.getElementById('mediaModal');
      const container = document.getElementById('mediaContainer');
      const metricsBar = document.getElementById('mediaMetricsBar');
      const encodingTag = document.getElementById('mediaEncodingTag');

      document.getElementById('mediaTitle').textContent = path.split('/').pop();
      encodingTag.textContent = ext.toUpperCase();
      metricsBar.innerHTML = '<span>⚡ Loading object & metrics...</span><span>--</span>';
      container.innerHTML = '<div style="color:var(--ink-secondary); padding: 40px; font-family:var(--font-mono); font-size:0.82rem;">Loading stream...</div>';
      modal.style.display = 'flex';

      const t0 = performance.now();

      if (['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp'].includes(ext)) {
        container.innerHTML = `<img src="${url}" style="max-width: 100%; max-height: 68vh; border-radius: 8px;">`;
        const dur = (performance.now() - t0).toFixed(1);
        metricsBar.innerHTML = `<span>⚡ Image View · Client Render: ${dur}ms</span><span>HTTP Stream</span>`;
      } else if (['mp4', 'webm', 'mov'].includes(ext)) {
        container.innerHTML = `<video src="${url}" controls autoplay style="max-width: 100%; max-height: 68vh; border-radius: 8px;"></video>`;
        metricsBar.innerHTML = `<span>⚡ HTML5 Media Stream · Range Enabled</span><span>HTTP 206 Partial</span>`;
      } else if (['txt', 'json', 'log', 'md', 'xml', 'csv', 'sql'].includes(ext)) {
        try {
          const resp = await api(`/api/files/${path}`);
          const serverTiming = resp.headers.get('Server-Timing') || 'local';
          const buf = await resp.arrayBuffer();
          const tNet = (performance.now() - t0).toFixed(1);

          const uint8 = new Uint8Array(buf);
          const decoded = decodeTextSmart(uint8);

          encodingTag.textContent = decoded.encoding;
          if (decoded.isBinary) {
            container.innerHTML = `
              <div style="width:100%;">
                <div style="color:var(--warning); font-size:0.78rem; font-family:var(--font-mono); margin-bottom:8px;">
                  ⚠️ Binary / Encrypted Data Detected (${decoded.encoding}). Displaying Hex Dump:
                </div>
                <pre>${escapeHtml(decoded.text)}</pre>
              </div>
            `;
          } else {
            let displayText = decoded.text;
            if (displayText.length > 250000) {
              displayText = displayText.substring(0, 250000) + '\n\n... [Content truncated for preview performance. Download for full file]';
            }
            container.innerHTML = `<pre>${escapeHtml(displayText)}</pre>`;
          }

          metricsBar.innerHTML = `
            <span>⚡ Server Time: <strong>${serverTiming}</strong> · Network RTT: <strong>${tNet}ms</strong></span>
            <span>Size: <strong>${formatBytes(buf.byteLength)}</strong></span>
          `;
        } catch (e) {
          container.innerHTML = `<div style="color: var(--danger)">Error reading file: ${e}</div>`;
        }
      } else {
        window.location.href = url;
        closeMediaModal();
      }
    }

    function escapeHtml(s) {
      return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    function closeMediaModal() {
      document.getElementById('mediaContainer').innerHTML = '';
      document.getElementById('mediaModal').style.display = 'none';
    }

    function copyURL(path) {
      navigator.clipboard.writeText(window.location.origin + path);
      showToast('Object link copied');
    }

    function openTokenModal() {
      document.getElementById('tokenInput').value = sessionToken;
      document.getElementById('tokenModal').style.display = 'flex';
    }
    function closeTokenModal() { document.getElementById('tokenModal').style.display = 'none'; }
    function saveToken() {
      sessionToken = document.getElementById('tokenInput').value.trim();
      localStorage.setItem('ostore_sess', sessionToken);
      closeTokenModal();
      showConsole();
    }

    function copySHA(sha) {
      if (!sha) return;
      navigator.clipboard.writeText(sha);
      showToast('SHA-256 copied to clipboard');
    }

    async function deleteItem(path, isDir) {
      if (!confirm(`Delete ${isDir ? 'folder' : 'file'} "${path}"?`)) return;
      const res = await api(`/api/files/${path}${isDir ? '?recursive=1' : ''}`, { method: 'DELETE' });
      if (res.ok) {
        showToast('Deleted successfully');
        loadDirectory(currentPath, false);
      }
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
      if (res.ok) {
        showToast('Folder created');
        loadDirectory(currentPath, false);
      }
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
      loadDirectory(currentPath, false);
    }

    function triggerFolderUploadInput() { document.getElementById('folderInput').click(); }
    async function handleFolderSelect(e) {
      const files = Array.from(e.target.files);
      if (!files.length) return;
      showToast(`Uploading folder tree (${files.length} files)...`);
      for (const f of files) {
        const relPath = f.webkitRelativePath || f.name;
        const dest = currentPath ? `${currentPath}/${relPath}` : relPath;
        await api(`/api/files/${dest}`, { method: 'PUT', body: f });
      }
      showToast('Folder tree uploaded successfully');
      e.target.value = '';
      loadDirectory(currentPath, false);
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

    // Initial Gatekeeper: Check Authentication
    async function checkAuthAndBoot() {
      if (!sessionToken) {
        showLogin();
        return;
      }

      try {
        const res = await api('/api/list?limit=1');
        if (res.ok) {
          showConsole();
        } else {
          showLogin();
        }
      } catch (e) {
        showLogin();
      }
    }

    checkAuthAndBoot();
  </script>
</body>
</html>
"##;
