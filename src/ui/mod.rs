//! Modern Single-Page Application (SPA) Web UI for ostore.
//!
//! Crafted to strictly adhere to Owner UI Taste & Operational Excellence:
//! - Apple-design restraint: Light theme (#F5F5F7, #1D1D1F, #0071E3).
//! - Unified Design Language: All top-level controls (status pill, sync btn, folder badge, search input)
//!   share the exact same 32px height and 980px pill radius for flawless visual alignment.
//! - Anti-Jitter Mobile Engine: table-layout:fixed, strictly targeted DOM patching (zero full-table wipes),
//!   hardware-accelerated GPU transitions (only opacity/transform, zero transition:all).
//! - Directory Dual-Metrics: Displays both Total Aggregated Size and Total Item Count.
//! - Complete Folder Capabilities: Download directory as tar, rename/move, inspect, sync, upload folder tree.
//! - Clean, popup-free login transition: zero unwanted OTP/token modals post-login.

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

pub async fn handler(headers: HeaderMap) -> Response {
    static ETAG: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(UI_HTML.as_bytes());
        format!("\"{:x}\"", hasher.finalize())
    });

    let etag = ETAG.as_str();

    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if if_none_match
            .to_str()
            .map(|s| s.trim() == etag || s.trim() == "*")
            .unwrap_or(false)
        {
            return (
                StatusCode::NOT_MODIFIED,
                [
                    (header::ETAG, etag),
                    (header::CACHE_CONTROL, "public, max-age=0, must-revalidate"),
                ],
            )
                .into_response();
        }
    }

    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::ETAG, etag),
            (header::CACHE_CONTROL, "public, max-age=0, must-revalidate"),
        ],
        UI_HTML,
    )
        .into_response()
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
      --radius-pill: 980px;
      --control-h: 32px;
      --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.04);
      --shadow-md: 0 4px 16px rgba(0, 0, 0, 0.05);
      --shadow-lg: 0 12px 36px rgba(0, 0, 0, 0.08);
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

    /* Universal Button Reset */
    button, .btn, .login-btn {
      -webkit-appearance: none;
      -moz-appearance: none;
      appearance: none;
      touch-action: manipulation;
      user-select: none;
      -webkit-user-select: none;
      font-family: inherit;
    }

    /* Unified Control Language: All top-bar controls share 32px height & 980px pill radius */
    .status-pill, .btn, .folder-stat-badge, .search-input {
      height: var(--control-h);
      border-radius: var(--radius-pill);
      border: 1px solid var(--card-border);
      box-sizing: border-box;
      display: inline-flex;
      align-items: center;
      vertical-align: middle;
      font-size: 0.78rem;
    }

    .status-pill {
      gap: 6px;
      padding: 0 12px;
      font-family: var(--font-mono);
      background: #FFFFFF;
      color: var(--ink-secondary);
      box-shadow: var(--shadow-sm);
    }

    .status-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--success);
      box-shadow: 0 0 6px rgba(52, 199, 89, 0.4);
      flex-shrink: 0;
    }

    .status-dot.disconnected {
      background: var(--danger);
      box-shadow: 0 0 6px rgba(255, 59, 48, 0.4);
    }

    .btn {
      justify-content: center;
      gap: 6px;
      padding: 0 14px;
      font-weight: 500;
      cursor: pointer;
      background: #FFFFFF;
      color: var(--ink);
      transition: background-color 0.15s ease, border-color 0.15s ease;
      box-shadow: var(--shadow-sm);
      white-space: nowrap;
    }

    .btn:hover {
      background: #F9F9FB;
      border-color: rgba(0, 0, 0, 0.16);
    }

    .btn:active {
      transform: scale(0.98);
    }

    .btn-primary {
      background: var(--accent);
      border-color: var(--accent);
      color: #FFFFFF;
      box-shadow: 0 2px 8px rgba(0, 113, 227, 0.22);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
      border-color: var(--accent-hover);
    }

    .search-input {
      background: #FFFFFF;
      padding: 0 14px;
      color: var(--ink);
      font-size: 16px; /* 16px prevents iOS focus zoom */
      width: 170px;
      outline: none;
      transition: border-color 0.15s ease, width 0.2s ease;
    }

    @media (min-width: 640px) {
      .search-input { font-size: 0.8rem; }
      .search-input:focus { width: 220px; border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft); }
    }

    .folder-stat-badge {
      font-family: var(--font-mono);
      padding: 0 12px;
      background: #FFFFFF;
      color: var(--ink-secondary);
      box-shadow: var(--shadow-sm);
      white-space: nowrap;
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

    /* 6-Digit OTP Box (Adaptive Flex, zero distortion) */
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
      height: 50px;
      font-family: var(--font-mono);
      font-size: 1.3rem;
      font-weight: 600;
      text-align: center;
      background: #F9F9FB;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      color: var(--ink);
      outline: none;
      transition: border-color 0.15s ease;
      -webkit-appearance: none;
    }

    .otp-digit:focus {
      border-color: var(--accent);
      background: #FFFFFF;
      box-shadow: 0 0 0 3px var(--accent-soft);
    }

    .login-btn {
      width: 100%;
      height: 44px;
      border-radius: var(--radius-pill);
      cursor: pointer;
      background: var(--accent);
      border: 1px solid var(--accent);
      color: #FFFFFF;
      font-size: 0.9rem;
      font-weight: 500;
      transition: background-color 0.15s ease;
      box-shadow: 0 2px 8px rgba(0, 113, 227, 0.25);
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .login-btn:hover { background: var(--accent-hover); }

    .login-footer-links {
      margin-top: 18px;
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
      .container { padding: 14px 10px; }
      .login-card { padding: 26px 16px; }
      .otp-digit { height: 46px; font-size: 1.15rem; }
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

    .panel {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--radius-md);
      backdrop-filter: blur(20px);
      box-shadow: var(--shadow-sm);
      margin-bottom: 24px;
      overflow: hidden;
      contain: layout paint;
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
      padding: 2px 8px;
      border-radius: var(--radius-pill);
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
      border-radius: var(--radius-pill);
      background: rgba(0, 0, 0, 0.06);
      display: flex;
      overflow: hidden;
      margin-bottom: 10px;
    }

    .spectrum-segment { height: 100%; }

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
      transition: color 0.12s ease;
    }

    .crumb-link:hover { color: var(--ink); }
    .crumb-sep { color: var(--ink-tertiary); font-size: 0.75rem; }

    /* Fixed Table Layout: Completely Eliminates Column Width Jitter & Reflows */
    .table-viewport {
      max-height: 580px;
      overflow-y: auto;
      overflow-x: auto;
      position: relative;
      -webkit-overflow-scrolling: touch;
      contain: paint;
    }

    .file-table {
      width: 100%;
      min-width: 660px;
      table-layout: fixed;
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
      height: 48px;
    }

    .file-row:hover { background: var(--card-hover); }

    .file-row td {
      padding: 8px 16px;
      vertical-align: middle;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .name-cell {
      display: flex;
      align-items: center;
      gap: 10px;
      cursor: pointer;
      font-weight: 500;
      overflow: hidden;
    }

    .name-cell span {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .name-cell svg { width: 16px; height: 16px; flex-shrink: 0; }
    .meta-mono { font-family: var(--font-mono); font-size: 0.76rem; color: var(--ink-secondary); }

    .dir-size-cell {
      display: flex;
      flex-direction: column;
      gap: 1px;
      cursor: pointer;
    }

    .dir-size-val { font-weight: 600; color: var(--ink); font-size: 0.82rem; }
    .dir-items-val { font-size: 0.69rem; color: var(--ink-secondary); }

    .node-badges-cluster {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      height: 24px;
      padding: 0 8px;
      border-radius: var(--radius-pill);
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      box-shadow: var(--shadow-sm);
      cursor: pointer;
      font-family: var(--font-mono);
      font-size: 0.72rem;
      white-space: nowrap;
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
      width: 28px;
      height: 28px;
      border-radius: var(--radius-pill);
      cursor: pointer;
      color: var(--ink-secondary);
      display: flex;
      align-items: center;
      justify-content: center;
      transition: background-color 0.12s ease;
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

    .cluster-sync-box { max-width: 640px; width: 95vw; }
    .fleet-nodes-strip {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: 8px;
      margin-bottom: 14px;
    }
    .fleet-node-item {
      padding: 10px 12px;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      background: #F9F9FB;
      cursor: pointer;
      user-select: none;
      transition: border-color 0.15s, background-color 0.15s;
    }
    .fleet-node-item:hover {
      border-color: var(--accent);
      background: #F4F6FC;
    }
    .fleet-node-item.selected {
      border-color: var(--accent);
      background: rgba(0, 102, 204, 0.05);
    }
    .sync-report-card {
      background: #F9F9FB;
      border: 1px solid var(--card-border);
      border-radius: var(--radius-sm);
      padding: 12px 14px;
      margin-bottom: 12px;
    }
    .sync-diff-row {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 6px 4px;
      border-bottom: 1px solid #F0F0F2;
    }
    .sync-diff-row:last-child {
      border-bottom: none;
    }
    .sync-diff-row:hover {
      background: #F8F9FA;
    }
    .diff-badge {
      display: inline-flex;
      align-items: center;
      font-size: 0.68rem;
      padding: 1px 6px;
      border-radius: 4px;
      font-weight: 600;
    }
    .diff-badge.missing {
      background: #FEE2E2;
      color: #DC2626;
    }
    .diff-badge.mismatch {
      background: #FEF3C7;
      color: #D97706;
    }
    .diff-badge.ok {
      background: #D1FAE5;
      color: #059669;
    }
    .mini-dot.probing { background: #D2D2D7; opacity: 0.6; }
    .ratio-badge.probing { background: rgba(0, 0, 0, 0.05); color: var(--ink-secondary); }

    .batch-action-bar {
      position: fixed;
      bottom: 24px;
      left: 50%;
      transform: translateX(-50%);
      background: rgba(255, 255, 255, 0.95);
      border: 1px solid var(--card-border);
      box-shadow: 0 12px 32px rgba(0, 0, 0, 0.16);
      border-radius: var(--radius-pill);
      padding: 8px 18px;
      display: none;
      align-items: center;
      gap: 12px;
      z-index: 100;
      backdrop-filter: blur(20px);
      -webkit-backdrop-filter: blur(20px);
    }
    .row-select-cb {
      width: 15px;
      height: 15px;
      cursor: pointer;
      accent-color: var(--accent);
      vertical-align: middle;
    }
    .parent-dir-row {
      transition: background-color 0.12s ease;
    }
    .parent-dir-row:hover {
      background: #F2F2F7 !important;
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
      align-items: flex-start;
      width: 100%;
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
      box-sizing: border-box;
    }

    .toast {
      position: fixed;
      bottom: 24px;
      right: 24px;
      background: #FFFFFF;
      border: 1px solid var(--card-border);
      padding: 8px 16px;
      border-radius: var(--radius-pill);
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
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*" autocomplete="one-time-code" autofocus>
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*" autocomplete="one-time-code">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*" autocomplete="one-time-code">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*" autocomplete="one-time-code">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*" autocomplete="one-time-code">
        <input type="text" class="otp-digit" maxlength="1" inputmode="numeric" pattern="[0-9]*" autocomplete="one-time-code">
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
          <div class="brand-subtitle">Distributed Object Storage · MD5 &amp; GDrive Cloud</div>
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
        <button class="btn" id="gdriveHeaderBtn" onclick="openGDriveModal()" title="Google Drive Cloud Storage Node">
          <svg width="14" height="14" viewBox="0 0 87.3 78">
            <path d="m6.6 66.85 3.85 6.65c.8 1.4 1.95 2.5 3.3 3.3l13.75-23.8h-27.5c0 1.55.4 3.1 1.2 4.5z" fill="#0066da"/>
            <path d="m43.65 25-13.75-23.8c-1.35.8-2.5 1.9-3.3 3.3l-25.4 44c-.8 1.4-1.2 2.95-1.2 4.5h27.5z" fill="#00ac47"/>
            <path d="m73.55 76.8c1.35-.8 2.5-1.9 3.3-3.3l1.6-2.75 7.65-13.25c.8-1.4 1.2-2.95 1.2-4.5h-27.502l5.852 11.5z" fill="#ea4335"/>
            <path d="m43.65 25 13.75-23.8c-1.35-.8-2.9-1.2-4.5-1.2h-18.5c-1.6 0-3.15.45-4.5 1.2z" fill="#00832d"/>
            <path d="m59.8 53h-32.3l-13.75 23.8c1.35.8 2.9 1.2 4.5 1.2h50.8c1.6 0 3.15-.45 4.5-1.2z" fill="#2684fc"/>
            <path d="m73.4 26.5-12.7-22c-.8-1.4-1.95-2.5-3.3-3.3l-13.75 23.8 16.15 28h27.45c0-1.55-.4-3.1-1.2-4.5z" fill="#ffba00"/>
          </svg>
          <span id="gdriveHeaderLabel">Google Drive</span>
        </button>
        <button class="btn btn-primary" onclick="openClusterSyncModal()" title="View cluster synchronization status & run fleet replication">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
          Cluster Sync
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
          <button class="btn" id="btnNavBack" onclick="navigateUp()" style="display: none; padding: 4px 9px; font-size: 0.78rem; align-items: center; gap: 4px;" title="返回上一级目录">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="15 18 9 12 15 6"></polyline></svg>
            <span>上一级</span>
          </button>
          <div class="breadcrumbs" id="breadcrumbs">
            <span class="crumb-link" onclick="navigate('')">root</span>
          </div>
          <span class="folder-stat-badge" id="folderStatBadge">0 items · 0 B</span>
        </div>
        <div style="display: flex; gap: 8px; align-items: center; flex-wrap: wrap;">
          <input type="text" class="search-input" id="searchBox" placeholder="Filter objects..." oninput="onFilter()">
          <button class="btn" id="btnSyncFolder" onclick="openCurrentFolderSync()" title="同步当前目录下的所有文件到指定节点">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
            <span>同步当前目录</span>
          </button>
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
              <th style="width: 38px; text-align: center;">
                <input type="checkbox" id="selectAllCb" class="row-select-cb" onchange="toggleSelectAllRows(this.checked)" title="全选当前列表项">
              </th>
              <th style="width: 38%;">Name</th>
              <th style="width: 15%;">Size & Total Items</th>
              <th style="width: 15%;">Modified</th>
              <th style="width: 18%;">Replication Map</th>
              <th style="width: 14%; text-align: right;">Actions</th>
            </tr>
          </thead>
          <tbody id="fileTableBody"></tbody>
        </table>
        <div id="scrollAnchor" style="height: 24px; text-align: center; color: var(--ink-secondary); font-size: 0.75rem; padding: 6px;"></div>
      </div>

      <!-- Floating Batch Action Bar -->
      <div class="batch-action-bar" id="batchActionBar">
        <span style="font-size: 0.82rem; font-weight: 600; color: var(--ink);" id="batchSelectedLabel">已选择 0 项</span>
        <button class="btn btn-primary" onclick="openBatchSync()" style="padding: 4px 12px; font-size: 0.78rem;">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
          批量同步到节点
        </button>
        <button class="btn" onclick="clearRowSelection()" style="padding: 4px 10px; font-size: 0.78rem;">取消选择</button>
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

  <!-- Cluster Fleet Synchronization & Status Modal -->
  <div class="backdrop" id="clusterSyncModal">
    <div class="dialog-box cluster-sync-box">
      <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;">
        <div style="display: flex; align-items: center; gap: 8px;">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
          <h3 style="margin: 0; font-size: 1.05rem;">Cluster Fleet Replication</h3>
        </div>
        <button class="icon-btn" onclick="closeClusterSyncModal()">✕</button>
      </div>

      <!-- Target Follower Nodes Selection -->
      <div style="margin-bottom: 12px;">
        <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom: 6px;">
          <label style="font-size: 0.78rem; font-weight: 600; color: var(--ink-secondary);">Target Nodes (Selectable Replicas):</label>
          <span style="font-size:0.72rem; color:var(--ink-tertiary);">Click node to toggle</span>
        </div>
        <div class="fleet-nodes-strip" id="syncFleetNodes">
          <div style="text-align: center; color: var(--ink-secondary); font-size: 0.8rem; padding: 8px;">Loading fleet nodes...</div>
        </div>
      </div>

      <!-- Target Scope Selector -->
      <div style="margin-bottom: 12px;">
        <label style="font-size: 0.78rem; font-weight: 600; color: var(--ink-secondary); margin-bottom: 6px; display: block;">Replication Scope:</label>
        <div style="display: flex; gap: 14px; font-size: 0.82rem; flex-wrap: wrap; align-items: center;">
          <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
            <input type="radio" name="syncScope" value="all" id="syncScopeAll" checked onchange="onSyncScopeChanged()">
            <span>Entire Storage (Root /)</span>
          </label>
          <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
            <input type="radio" name="syncScope" value="current" id="syncScopeCurrent" onchange="onSyncScopeChanged()">
            <span id="syncCurrentFolderLabel">Current Folder</span>
          </label>
          <label style="display: none; align-items: center; gap: 6px; cursor: pointer;" id="syncItemScopeWrapper">
            <input type="radio" name="syncScope" value="item" id="syncScopeItem" onchange="onSyncScopeChanged()">
            <span id="syncItemScopeLabel" style="font-weight: 600; color: var(--accent);">Target Item</span>
          </label>
        </div>
        <div id="syncTargetBadge" style="display: none; margin-top: 6px; font-size: 0.74rem; font-family: var(--font-mono); color: var(--accent); background: rgba(0, 113, 227, 0.08); padding: 4px 8px; border-radius: 4px; word-break: break-all;"></div>
      </div>

      <!-- Sync Status & Last Report Box -->
      <div class="sync-report-card" id="syncReportCard">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
          <span style="font-weight: 600; font-size: 0.82rem; color: var(--ink);">Fleet Sync Status</span>
          <span class="ratio-badge full" id="syncStatusBadge">Ready</span>
        </div>
        <div id="syncReportContent" style="font-size: 0.78rem; color: var(--ink-secondary); line-height: 1.5; font-family: var(--font-mono);">
          Loading replication report...
        </div>
      </div>

      <!-- Interactive Differences Panel (Checklist) -->
      <div id="syncDiffPanel" style="display: none; margin-bottom: 14px; border: 1px solid var(--card-border); border-radius: var(--radius-sm); padding: 10px; background: #FFFFFF;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
          <span style="font-weight: 600; font-size: 0.82rem; color: var(--ink);">Detected Differences (<span id="syncDiffCount">0</span>)</span>
          <div style="display: flex; gap: 10px; font-size: 0.78rem;">
            <a href="javascript:void(0)" onclick="selectAllDiffs(true)" style="color:var(--accent); text-decoration:none;">Select All</a>
            <a href="javascript:void(0)" onclick="selectAllDiffs(false)" style="color:var(--ink-secondary); text-decoration:none;">Deselect All</a>
          </div>
        </div>
        <div id="syncDiffList" style="max-height: 160px; overflow-y: auto; font-size: 0.76rem; font-family: var(--font-mono);"></div>
      </div>

      <!-- Live Replication Progress Bar -->
      <div id="syncLiveProgressContainer" style="display: none; margin-bottom: 14px;">
        <div style="display: flex; justify-content: space-between; font-size: 0.78rem; margin-bottom: 4px; font-family: var(--font-mono);">
          <span id="syncLiveStatusText" style="color: var(--ink); font-weight: 600;">Replicating...</span>
          <span id="syncLivePercentText" style="color: var(--accent); font-weight: 600;">0%</span>
        </div>
        <div style="width: 100%; height: 8px; background: #E5E5EA; border-radius: 4px; overflow: hidden;">
          <div id="syncLiveProgressBar" style="width: 0%; height: 100%; background: var(--accent); transition: width 0.25s ease;"></div>
        </div>
        <div id="syncLiveSubText" style="font-size: 0.72rem; color: var(--ink-secondary); margin-top: 4px; font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;"></div>
      </div>

      <!-- Action Buttons -->
      <div style="display: flex; justify-content: space-between; align-items: center; margin-top: 14px;">
        <button class="btn" id="btnCancelSync" onclick="cancelRunningSync()" style="display: none; color: var(--danger); border-color: rgba(255, 59, 48, 0.25);">Cancel Sync</button>
        <span id="syncProgressTimer" style="font-family: var(--font-mono); font-size: 0.78rem; color: var(--ink-secondary);"></span>
        <div style="display: flex; gap: 8px; margin-left: auto;">
          <button class="btn" onclick="closeClusterSyncModal()">Close</button>
          <button class="btn" id="btnScanDiff" onclick="scanSyncDiff()" title="Scan and preview mismatched files without altering remote storage">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
            <span>Scan Differences</span>
          </button>
          <button class="btn btn-primary" id="btnRunSync" onclick="runClusterSyncFromModal()">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
            <span id="btnRunSyncText">Run Sync Now</span>
          </button>
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
        <div style="display:flex; align-items:center; gap:8px; min-width:0; overflow:hidden;">
          <span id="mediaIcon" style="display:inline-flex; align-items:center; width:16px; height:16px; flex-shrink:0;"></span>
          <span id="mediaTitle" style="font-weight:600; font-size:0.9rem; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;"></span>
          <span id="mediaEncodingTag" style="font-family:var(--font-mono); font-size:0.7rem; padding:2px 8px; border-radius:var(--radius-pill); background:rgba(0,0,0,0.05); flex-shrink:0;"></span>
        </div>
        <div style="display:flex; align-items:center; gap:6px; flex-shrink:0;">
          <button id="mediaCopyBtn" class="icon-btn" title="Copy text content" onclick="copyCurrentPreviewText()" style="display:none;">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
          </button>
          <a id="mediaDownloadBtn" class="icon-btn" title="Download object" href="#" download>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
          </a>
          <button class="icon-btn" title="Close" onclick="closeMediaModal()">✕</button>
        </div>
      </div>
      <div class="media-container" id="mediaContainer"></div>
      <div class="metrics-bar" id="mediaMetricsBar">
        <span id="mediaServerTiming">Loading metrics...</span>
        <span id="mediaTransferSpeed">--</span>
      </div>
    </div>
  </div>

  <!-- Google Drive Cloud Storage Modal -->
  <div class="backdrop" id="gdriveModal">
    <div class="dialog-box" style="max-width: 520px;">
      <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px;">
        <div style="display: flex; align-items: center; gap: 8px;">
          <svg width="20" height="20" viewBox="0 0 87.3 78">
            <path d="m6.6 66.85 3.85 6.65c.8 1.4 1.95 2.5 3.3 3.3l13.75-23.8h-27.5c0 1.55.4 3.1 1.2 4.5z" fill="#0066da"/>
            <path d="m43.65 25-13.75-23.8c-1.35.8-2.5 1.9-3.3 3.3l-25.4 44c-.8 1.4-1.2 2.95-1.2 4.5h27.5z" fill="#00ac47"/>
            <path d="m73.55 76.8c1.35-.8 2.5-1.9 3.3-3.3l1.6-2.75 7.65-13.25c.8-1.4 1.2-2.95 1.2-4.5h-27.502l5.852 11.5z" fill="#ea4335"/>
            <path d="m43.65 25 13.75-23.8c-1.35-.8-2.9-1.2-4.5-1.2h-18.5c-1.6 0-3.15.45-4.5 1.2z" fill="#00832d"/>
            <path d="m59.8 53h-32.3l-13.75 23.8c1.35.8 2.9 1.2 4.5 1.2h50.8c1.6 0 3.15-.45 4.5-1.2z" fill="#2684fc"/>
            <path d="m73.4 26.5-12.7-22c-.8-1.4-1.95-2.5-3.3-3.3l-13.75 23.8 16.15 28h27.45c0-1.55-.4-3.1-1.2-4.5z" fill="#ffba00"/>
          </svg>
          <h3 style="margin: 0; font-size: 1.05rem;">Google Drive 云存储节点</h3>
        </div>
        <button class="icon-btn" onclick="closeGDriveModal()">✕</button>
      </div>

      <!-- Connected State Banner -->
      <div id="gdriveConnectedCard" style="display: none; background: rgba(52, 199, 89, 0.08); border: 1px solid rgba(52, 199, 89, 0.25); border-radius: 12px; padding: 14px 16px; margin-bottom: 16px;">
        <div style="display: flex; justify-content: space-between; align-items: center;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <span class="status-dot"></span>
            <strong style="font-size: 0.88rem; color: #1D1D1F;">已连接 Google Drive</strong>
          </div>
          <span class="ratio-badge full" id="gdriveQuotaText">-- GB / -- GB</span>
        </div>
        <div style="font-size: 0.74rem; font-family: var(--font-mono); color: var(--ink-secondary); margin-top: 6px;" id="gdriveClientText">
          Client ID: --
        </div>
        <div style="margin-top: 10px; display: flex; justify-content: flex-end;">
          <button class="btn btn-delete" style="font-size: 0.76rem; height: 26px; padding: 0 10px;" onclick="disconnectGDrive()">断开连接</button>
        </div>
      </div>

      <!-- Connect Form Tabs -->
      <div id="gdriveConnectTabs">
        <div style="display: flex; border-bottom: 1px solid var(--card-border); margin-bottom: 16px; gap: 16px;">
          <button id="tabOAuthBtn" onclick="switchGDriveTab('oauth')" style="border:none; background:none; padding: 6px 0; font-size: 0.82rem; font-weight:600; color: var(--accent); border-bottom: 2px solid var(--accent); cursor: pointer;">
            网页一键授权 (OAuth)
          </button>
          <button id="tabManualBtn" onclick="switchGDriveTab('manual')" style="border:none; background:none; padding: 6px 0; font-size: 0.82rem; font-weight:500; color: var(--ink-secondary); border-bottom: 2px solid transparent; cursor: pointer;">
            填入凭据 (Credentials)
          </button>
        </div>

        <!-- OAuth Tab Pane -->
        <div id="paneOAuth">
          <p style="font-size: 0.78rem; color: var(--ink-secondary); line-height: 1.5; margin-bottom: 12px;">
            填入您在 Google Cloud 创建的 OAuth Client ID 与 Client Secret，点击下方按钮将自动唤起 Google 授权页。
          </p>
          <div style="display: flex; flex-direction: column; gap: 10px; margin-bottom: 16px;">
            <div>
              <label style="display: block; font-size: 0.74rem; font-weight:600; color: var(--ink-secondary); margin-bottom: 4px;">Client ID</label>
              <input type="text" id="gdOAuthClientId" placeholder="xxxx.apps.googleusercontent.com" style="width: 100%; height: 34px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--card-border); font-size: 0.8rem; font-family: var(--font-mono); box-sizing: border-box;">
            </div>
            <div>
              <label style="display: block; font-size: 0.74rem; font-weight:600; color: var(--ink-secondary); margin-bottom: 4px;">Client Secret</label>
              <input type="password" id="gdOAuthClientSecret" placeholder="GOCSPX-xxxx" style="width: 100%; height: 34px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--card-border); font-size: 0.8rem; font-family: var(--font-mono); box-sizing: border-box;">
            </div>
          </div>
          <button class="btn btn-primary" style="width: 100%; height: 36px; justify-content: center;" onclick="startGDriveOAuth()">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><polyline points="10 17 15 12 10 7"/><line x1="15" y1="12" x2="3" y2="12"/></svg>
            前往 Google 授权登录
          </button>
        </div>

        <!-- Manual Tab Pane -->
        <div id="paneManual" style="display: none;">
          <p style="font-size: 0.78rem; color: var(--ink-secondary); line-height: 1.5; margin-bottom: 12px;">
            如果您已有包含 Refresh Token 的凭据（如使用 Rclone authorize 生成）：
          </p>
          <div style="display: flex; flex-direction: column; gap: 10px; margin-bottom: 16px;">
            <div>
              <label style="display: block; font-size: 0.74rem; font-weight:600; color: var(--ink-secondary); margin-bottom: 4px;">Client ID</label>
              <input type="text" id="gdManualClientId" placeholder="xxxx.apps.googleusercontent.com" style="width: 100%; height: 34px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--card-border); font-size: 0.8rem; font-family: var(--font-mono); box-sizing: border-box;">
            </div>
            <div>
              <label style="display: block; font-size: 0.74rem; font-weight:600; color: var(--ink-secondary); margin-bottom: 4px;">Client Secret</label>
              <input type="password" id="gdManualClientSecret" placeholder="GOCSPX-xxxx" style="width: 100%; height: 34px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--card-border); font-size: 0.8rem; font-family: var(--font-mono); box-sizing: border-box;">
            </div>
            <div>
              <label style="display: block; font-size: 0.74rem; font-weight:600; color: var(--ink-secondary); margin-bottom: 4px;">Refresh Token</label>
              <input type="password" id="gdManualRefreshToken" placeholder="1//xxxx" style="width: 100%; height: 34px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--card-border); font-size: 0.8rem; font-family: var(--font-mono); box-sizing: border-box;">
            </div>
          </div>
          <button class="btn btn-primary" style="width: 100%; height: 36px; justify-content: center;" onclick="submitManualGDrive()">
            验证凭据并保存
          </button>
        </div>

        <!-- Google Guide Details Accordion -->
        <details style="margin-top: 14px; font-size: 0.75rem; color: var(--ink-secondary); border-top: 1px solid var(--card-border); padding-top: 10px;">
          <summary style="cursor: pointer; font-weight: 600; color: var(--accent);">如何获取 Google Client ID & Secret？（3步教程）</summary>
          <div style="line-height: 1.6; margin-top: 8px;">
            1. 登录 <a href="https://console.cloud.google.com/" target="_blank" style="color:var(--accent);">Google Cloud Console</a> 新建一个项目。<br>
            2. 在「API 和服务」→「库」中搜索 <strong>Google Drive API</strong> 并点击启用。<br>
            3. 在「OAuth 同意屏幕」将类型选为外部，填写应用名并添加自己的 Google 邮箱为测试用户。<br>
            4. 在「凭据」→「创建凭据」选择 <strong>OAuth 客户端 ID</strong>，应用类型选「Web 应用」。<br>
            5. 在「已获授权的重定向 URI」中添加（必须与当前访问域名 100% 一致）：<br>
            <div style="display:flex; align-items:center; gap:6px; margin: 6px 0;">
              <code id="gdriveCallbackUriCode" style="flex:1; background:rgba(0,0,0,0.05); padding: 5px 8px; border-radius: 4px; word-break: break-all; font-size: 0.72rem; font-family: var(--font-mono);">https://xyz69.top/ostore/api/cluster/gdrive/callback</code>
              <button type="button" class="btn" style="height:26px; font-size:0.7rem; padding: 0 8px;" onclick="copyGDriveCallbackUri()">复制</button>
            </div>
            6. 复制生成的 Client ID 与 Client Secret 粘贴至上方即可。
          </div>
        </details>

        <!-- Troubleshooting FAQ -->
        <details style="margin-top: 10px; font-size: 0.75rem; color: var(--ink-secondary); border-top: 1px solid var(--card-border); padding-top: 10px;">
          <summary style="cursor: pointer; font-weight: 600; color: #FF3B30;">⚠️ 连接常见报错与排查解决 (点击展开)</summary>
          <div style="line-height: 1.6; margin-top: 8px;">
            <div style="margin-bottom: 8px;">
              <strong style="color:var(--ink);">1. Error 400: redirect_uri_mismatch</strong><br>
              说明 Google Cloud 控制台填写的重定向 URI 与当前访问地址不匹配。点击上方「复制」按钮获取准确的重定向 URI，填入 Google 控制台「已获授权的重定向 URI」中。
            </div>
            <div style="margin-bottom: 8px;">
              <strong style="color:var(--ink);">2. Access blocked / Error 403: access_denied</strong><br>
              说明应用处于测试中，未添加授权账号。请在 Google Cloud Console →「OAuth 同意屏幕」→「测试用户 (Test users)」中添加您正在登录的 Google 邮箱。
            </div>
            <div style="margin-bottom: 8px;">
              <strong style="color:var(--ink);">3. Google 提示「Google 尚未验证此应用」</strong><br>
              自建 OAuth 应用的标准安全提示，并非报错。点击左下角「高级 (Advanced)」→「前往...（不安全）」→ 勾选 Drive 权限后继续。
            </div>
            <div style="margin-bottom: 8px;">
              <strong style="color:var(--ink);">4. Google Drive API 403 (API not enabled)</strong><br>
              请在 Google Cloud Console →「API 和服务」→「库」中搜索启用 <strong>Google Drive API</strong>。
            </div>
            <div>
              <strong style="color:var(--ink);">5. 缺少 Refresh Token</strong><br>
              该账号此前已对本应用授权过。前往 <a href="https://myaccount.google.com/connections" target="_blank" style="color:var(--accent);">Google 账号关联应用</a> 解除授权后重新点击授权，或在「手动填入凭据」标签页输入已有 Refresh Token。
            </div>
          </div>
        </details>
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
    const dirCache = new Map(); // path -> { entries, totalCount, totalBytes, hasMore, offset, timestamp }
    const DIR_CACHE_TTL_MS = 60000;

    // Row selection for granular batch operations
    const selectedRowPaths = new Set();
    let activeSyncTarget = null; // null or { type: 'item'|'dir'|'file'|'batch'|'current', paths: [...], prefix: '...', label: '...' }

    let sessionToken = localStorage.getItem('ostore_sess') || localStorage.getItem('ostore_token') || '';
    let eventSource = null;

    function getBasePath() {
      return window.location.pathname.startsWith('/ostore') ? '/ostore' : '';
    }

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

    let currentPreviewText = '';
    let currentPreviewUrl = '';

    function escapeHtml(s) {
      if (s === null || s === undefined) return '';
      return String(s)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
    }

    function escapeJs(s) {
      if (s === null || s === undefined) return '';
      return String(s).replace(/\\/g, '\\\\').replace(/'/g, "\\'");
    }

    function copyCurrentPreviewText() {
      if (!currentPreviewText) return;
      navigator.clipboard.writeText(currentPreviewText);
      showToast('Object content copied');
    }

    function stripBackupSuffix(name) {
      if (!name) return '';
      const backupRegex = /\.(?:bak|backup|old|orig|original|save|saved|tmp|temp|swp|dist|default|example|sample|template|tpl|\d{8}(?:[-_]\d{4,6})?|\d{10,14}|\d+)$/i;
      if (backupRegex.test(name)) {
        return name.replace(backupRegex, '');
      }
      return name;
    }

    function getFileCategory(name) {
      if (!name) return { category: 'file', ext: '', tag: 'FILE' };
      const clean = stripBackupSuffix(name);
      const lowerClean = clean.toLowerCase();
      const dotIdx = clean.lastIndexOf('.');
      const ext = dotIdx >= 0 ? clean.substring(dotIdx + 1).toLowerCase() : '';

      // Special configuration & build filenames
      if (lowerClean === 'caddyfile' || lowerClean.startsWith('caddyfile.') || lowerClean.endsWith('.caddyfile')) {
        return { category: 'code', ext: 'caddyfile', tag: 'CADDYFILE' };
      }
      if (lowerClean === 'dockerfile' || lowerClean.startsWith('dockerfile.') || lowerClean === 'containerfile') {
        return { category: 'code', ext: 'dockerfile', tag: 'DOCKER' };
      }
      if (['makefile', 'gnumakefile', 'justfile', 'earthfile', 'taskfile', 'procfile', 'gemfile', 'rakefile', 'vagrantfile', 'brewfile'].includes(lowerClean)) {
        return { category: 'code', ext: ext || lowerClean, tag: 'BUILD' };
      }
      if (['hosts', 'resolv.conf', 'crontab', 'sudoers', 'fstab'].includes(lowerClean) || lowerClean.startsWith('.env')) {
        return { category: 'code', ext: ext || 'env', tag: 'CONFIG' };
      }
      if (['license', 'copying', 'authors', 'changelog', 'changes', 'readme', 'todo', 'notice'].includes(lowerClean)) {
        return { category: 'text', ext: ext || 'txt', tag: 'DOC' };
      }
      if (['.gitignore', '.gitattributes', '.gitmodules', '.dockerignore', '.editorconfig', '.npmrc', '.zshrc', '.bashrc', '.profile', '.tmux.conf', '.prettierrc', '.eslintrc', '.babelrc'].includes(lowerClean)) {
        return { category: 'code', ext: ext || 'conf', tag: 'CONFIG' };
      }

      // Compound archive extensions
      if (lowerClean.endsWith('.tar.gz') || lowerClean.endsWith('.tgz')) return { category: 'archive', ext: 'tar.gz', tag: 'TAR.GZ' };
      if (lowerClean.endsWith('.tar.bz2') || lowerClean.endsWith('.tbz2')) return { category: 'archive', ext: 'tar.bz2', tag: 'TAR.BZ2' };
      if (lowerClean.endsWith('.tar.xz') || lowerClean.endsWith('.txz')) return { category: 'archive', ext: 'tar.xz', tag: 'TAR.XZ' };

      // Images
      if (['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'svg', 'ico', 'avif', 'tif', 'tiff'].includes(ext)) {
        return { category: 'image', ext, tag: ext.toUpperCase() };
      }
      // Videos
      if (['mp4', 'webm', 'mov', 'mkv', 'avi', 'm4v', 'ogv'].includes(ext)) {
        return { category: 'video', ext, tag: ext.toUpperCase() };
      }
      // Audio
      if (['mp3', 'wav', 'ogg', 'aac', 'flac', 'm4a', 'opus', 'weba'].includes(ext)) {
        return { category: 'audio', ext, tag: ext.toUpperCase() };
      }
      // PDF
      if (ext === 'pdf') {
        return { category: 'pdf', ext, tag: 'PDF' };
      }
      // Archives
      if (['zip', 'tar', 'gz', 'tgz', 'bz2', 'tbz2', 'xz', 'txz', 'zst', '7z', 'rar', 'iso', 'dmg', 'pkg', 'deb', 'rpm'].includes(ext)) {
        return { category: 'archive', ext, tag: ext.toUpperCase() };
      }
      // Executables & binary bytecode/fonts
      if (['exe', 'dll', 'so', 'dylib', 'a', 'o', 'bin', 'wasm', 'class', 'pyc', 'woff', 'woff2', 'ttf', 'otf', 'eot'].includes(ext)) {
        return { category: 'binary', ext, tag: ext.toUpperCase() };
      }
      // Code & Scripts
      if (['rs', 'go', 'py', 'js', 'mjs', 'cjs', 'ts', 'tsx', 'jsx', 'c', 'cpp', 'cc', 'cxx', 'h', 'hpp', 'java', 'kt', 'kts', 'scala', 'rb', 'php', 'lua', 'pl', 'pm', 'r', 'swift', 'dart', 'cs', 'fs', 'sh', 'bash', 'zsh', 'fish', 'sql', 'proto', 'graphql', 'gql', 'vue', 'svelte', 'diff', 'patch'].includes(ext)) {
        return { category: 'code', ext, tag: ext.toUpperCase() };
      }
      // Config & Markup
      if (['caddyfile', 'caddy', 'toml', 'yaml', 'yml', 'json', 'json5', 'jsonc', 'jsonl', 'ndjson', 'ini', 'conf', 'cnf', 'cfg', 'properties', 'env', 'xml', 'html', 'htm', 'css', 'scss', 'sass', 'less', 'service', 'unit', 'rules'].includes(ext)) {
        return { category: 'code', ext, tag: ext.toUpperCase() };
      }
      // Text & Docs
      if (['txt', 'text', 'log', 'md', 'markdown', 'rst', 'adoc', 'org', 'csv', 'tsv', 'lock', 'sum', 'mod'].includes(ext)) {
        return { category: 'text', ext, tag: ext.toUpperCase() };
      }

      return { category: 'unknown', ext: ext || '', tag: (ext || 'OBJECT').toUpperCase() };
    }

    function getFileIcon(name) {
      const { category } = getFileCategory(name);
      if (category === 'code') {
        return `<svg viewBox="0 0 24 24" fill="none" stroke="#5E5CE6" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>`;
      }
      if (category === 'text') {
        return `<svg viewBox="0 0 24 24" fill="none" stroke="#34C759" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>`;
      }
      if (category === 'image') {
        return `<svg viewBox="0 0 24 24" fill="none" stroke="#FF9500" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>`;
      }
      if (category === 'video') {
        return `<svg viewBox="0 0 24 24" fill="none" stroke="#FF2D55" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>`;
      }
      if (category === 'audio') {
        return `<svg viewBox="0 0 24 24" fill="none" stroke="#AF52DE" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>`;
      }
      if (category === 'pdf') {
        return `<svg viewBox="0 0 24 24" fill="none" stroke="#FF3B30" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="9" y1="13" x2="15" y2="13"/></svg>`;
      }
      if (category === 'archive') {
        return `<svg viewBox="0 0 24 24" fill="none" stroke="#8E8E93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/><line x1="10" y1="12" x2="14" y2="12"/></svg>`;
      }
      return `<svg viewBox="0 0 24 24" fill="none" stroke="#86868B" stroke-width="1.5"><path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>`;
    }

    async function api(path, opts = {}) {
      const url = `${getBasePath()}${path}`;
      opts.headers = opts.headers || {};
      const tokenUsed = sessionToken;
      if (tokenUsed) opts.headers['Authorization'] = `Bearer ${tokenUsed}`;
      const res = await fetch(url, opts);
      if (res.status === 401) {
        // Only trigger logout if the 401 corresponds to our current active token
        if (tokenUsed && tokenUsed === sessionToken) {
          sessionToken = '';
          localStorage.removeItem('ostore_sess');
          showLogin();
        }
      }
      return res;
    }

    // Authenticator OTP UI & Flow
    let isSubmittingOtp = false;
    const otpInputs = document.querySelectorAll('.otp-digit');
    otpInputs.forEach((input, index) => {
      input.addEventListener('input', (e) => {
        const raw = e.target.value.replace(/\D/g, '');
        if (raw.length > 1) {
          // Multi-character input (e.g. browser autofill, SMS/Authenticator suggestion, or paste)
          const digits = raw.slice(0, 6);
          digits.split('').forEach((d, i) => {
            if (otpInputs[i]) otpInputs[i].value = d;
          });
          const nextIndex = Math.min(digits.length, otpInputs.length - 1);
          otpInputs[nextIndex].focus();
          checkAutoSubmit();
          return;
        }

        e.target.value = raw;
        if (raw.length === 1 && index < otpInputs.length - 1) {
          otpInputs[index + 1].focus();
        }
        checkAutoSubmit();
      });

      input.addEventListener('keydown', (e) => {
        if (e.key === 'Backspace') {
          if (!e.target.value && index > 0) {
            otpInputs[index - 1].focus();
            otpInputs[index - 1].value = '';
          }
        } else if (e.key === 'Enter') {
          submitOtpLogin();
        }
      });

      input.addEventListener('paste', (e) => {
        e.preventDefault();
        const text = (e.clipboardData || window.clipboardData).getData('text') || '';
        const digits = text.replace(/\D/g, '').slice(0, 6);
        if (digits.length > 0) {
          digits.split('').forEach((digit, i) => {
            if (otpInputs[i]) otpInputs[i].value = digit;
          });
          const focusIdx = Math.min(digits.length, 5);
          otpInputs[focusIdx].focus();
          if (digits.length === 6) {
            submitOtpLogin();
          }
        }
      });
    });

    function checkAutoSubmit() {
      const code = Array.from(otpInputs).map(i => i.value.trim()).join('');
      if (code.length === 6 && /^\d{6}$/.test(code)) {
        submitOtpLogin();
      }
    }

    async function submitOtpLogin() {
      if (isSubmittingOtp) return;

      const code = Array.from(otpInputs).map(i => i.value.trim()).join('');
      if (code.length !== 6 || !/^\d{6}$/.test(code)) {
        showToast('Please enter a 6-digit Authenticator code');
        return;
      }

      isSubmittingOtp = true;
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
          closeAllModals();
          showToast('Authenticator verified');
          showConsole();
        } else {
          showToast('Invalid verification code. Check Authenticator app.');
          otpInputs.forEach(i => i.value = '');
          otpInputs[0].focus();
        }
      } catch (e) {
        showToast('Authentication error: ' + (e.message || e));
      } finally {
        isSubmittingOtp = false;
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
        showToast('Failed to load setup info: ' + e);
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
      closeAllModals();
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
      document.getElementById('loginView').style.display = 'flex';
      document.getElementById('mainConsole').style.display = 'none';
      otpInputs.forEach(i => i.value = '');
      otpInputs[0].focus();
    }

    function showConsole() {
      closeAllModals();
      isLoading = false;
      document.getElementById('loginView').style.display = 'none';
      document.getElementById('mainConsole').style.display = 'block';
      initSSE();
      loadTopology();
      checkGDriveStatus();

      const searchParams = new URLSearchParams(window.location.search);
      let initialPath = searchParams.get('path');
      if (initialPath === null) {
        initialPath = sessionStorage.getItem('ostore_current_path') || '';
      }
      loadDirectory(initialPath, false, false);
    }

    function closeAllModals() {
      document.getElementById('totpSetupModal').style.display = 'none';
      document.getElementById('tokenModal').style.display = 'none';
      document.getElementById('folderDetailModal').style.display = 'none';
      document.getElementById('clusterSyncModal').style.display = 'none';
      document.getElementById('fileTopoModal').style.display = 'none';
      document.getElementById('mediaModal').style.display = 'none';
      const gdModal = document.getElementById('gdriveModal');
      if (gdModal) gdModal.style.display = 'none';
    }

    function openGDriveModal() {
      checkGDriveStatus();
      updateGDriveCallbackUri();
      document.getElementById('gdriveModal').style.display = 'flex';
    }

    function updateGDriveCallbackUri() {
      const el = document.getElementById('gdriveCallbackUriCode');
      if (el) {
        el.textContent = window.location.origin + getBasePath() + '/api/cluster/gdrive/callback';
      }
    }

    function copyGDriveCallbackUri() {
      const uri = window.location.origin + getBasePath() + '/api/cluster/gdrive/callback';
      navigator.clipboard.writeText(uri).then(() => {
        showToast('回调地址已复制到剪贴板');
      }).catch(() => {
        showToast('复制失败，请手动长按复制');
      });
    }

    function closeGDriveModal() {
      document.getElementById('gdriveModal').style.display = 'none';
    }

    function switchGDriveTab(tab) {
      const paneOAuth = document.getElementById('paneOAuth');
      const paneManual = document.getElementById('paneManual');
      const tabOAuthBtn = document.getElementById('tabOAuthBtn');
      const tabManualBtn = document.getElementById('tabManualBtn');

      if (tab === 'oauth') {
        paneOAuth.style.display = 'block';
        paneManual.style.display = 'none';
        tabOAuthBtn.style.color = 'var(--accent)';
        tabOAuthBtn.style.borderBottom = '2px solid var(--accent)';
        tabManualBtn.style.color = 'var(--ink-secondary)';
        tabManualBtn.style.borderBottom = '2px solid transparent';
      } else {
        paneOAuth.style.display = 'none';
        paneManual.style.display = 'block';
        tabManualBtn.style.color = 'var(--accent)';
        tabManualBtn.style.borderBottom = '2px solid var(--accent)';
        tabOAuthBtn.style.color = 'var(--ink-secondary)';
        tabOAuthBtn.style.borderBottom = '2px solid transparent';
      }
    }

    async function checkGDriveStatus() {
      try {
        const res = await api('/api/cluster/gdrive/status');
        if (!res.ok) return;
        const data = await res.json();
        const headerLabel = document.getElementById('gdriveHeaderLabel');
        const card = document.getElementById('gdriveConnectedCard');
        const tabs = document.getElementById('gdriveConnectTabs');
        const quota = document.getElementById('gdriveQuotaText');
        const clientText = document.getElementById('gdriveClientText');

        if (data && data.connected) {
          const used = formatBytes(data.disk_total_bytes - data.disk_available_bytes);
          const total = formatBytes(data.disk_total_bytes);
          const btn = document.getElementById('gdriveHeaderBtn');
          if (btn) {
            btn.style.background = 'rgba(52, 199, 89, 0.12)';
            btn.style.borderColor = 'rgba(52, 199, 89, 0.35)';
            btn.title = `Google Drive 云盘已就绪 (${total} 总量，${formatBytes(data.disk_available_bytes)} 可用)`;
          }
          if (headerLabel) {
            headerLabel.innerHTML = `<span class="status-dot" style="margin-right:4px;"></span>GDrive (${total})`;
          }
          if (card) card.style.display = 'block';
          if (tabs) tabs.style.display = 'none';
          if (quota) quota.textContent = `${used} / ${total}`;
          if (clientText) clientText.textContent = `Client: ${data.client_id} · Root: ${data.root_folder_id}`;
        } else {
          const btn = document.getElementById('gdriveHeaderBtn');
          if (btn) {
            btn.style.background = '';
            btn.style.borderColor = '';
            btn.title = 'Google Drive Cloud Storage Node';
          }
          if (headerLabel) headerLabel.textContent = 'Google Drive';
          if (card) card.style.display = 'none';
          if (tabs) tabs.style.display = 'block';
        }
      } catch (e) {
        console.error('Failed to check GDrive status', e);
      }
    }

    async function startGDriveOAuth() {
      const clientId = document.getElementById('gdOAuthClientId').value.trim();
      const clientSecret = document.getElementById('gdOAuthClientSecret').value.trim();
      if (!clientId || !clientSecret) {
        showToast('请完整输入 Client ID 与 Client Secret');
        return;
      }
      try {
        const redirectUri = window.location.origin + getBasePath() + '/api/cluster/gdrive/callback';
        const res = await api('/api/cluster/gdrive/auth-url', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ client_id: clientId, client_secret: clientSecret, redirect_uri: redirectUri })
        });
        const data = await res.json();
        if (data && data.auth_url) {
          showToast('正在前往 Google 授权页面...');
          window.location.href = data.auth_url;
        } else {
          showToast('获取授权链接失败: ' + (data.error || '未知错误'));
        }
      } catch (e) {
        showToast('获取授权链接失败: ' + (e.error || e.message));
      }
    }

    async function submitManualGDrive() {
      const clientId = document.getElementById('gdManualClientId').value.trim();
      const clientSecret = document.getElementById('gdManualClientSecret').value.trim();
      const refreshToken = document.getElementById('gdManualRefreshToken').value.trim();
      if (!clientId || !clientSecret || !refreshToken) {
        showToast('请完整输入 Client ID, Client Secret 和 Refresh Token');
        return;
      }
      try {
        const res = await api('/api/cluster/gdrive/connect', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ client_id: clientId, client_secret: clientSecret, refresh_token: refreshToken })
        });
        const data = await res.json();
        if (res.ok) {
          showToast('Google Drive 接入成功！');
          checkGDriveStatus();
          loadTopology();
        } else {
          showToast('接入失败: ' + (data.error || '凭据无效'));
        }
      } catch (e) {
        showToast('接入失败: ' + (e.error || e.message));
      }
    }

    async function disconnectGDrive() {
      if (!confirm('确定断开与 Google Drive 的连接吗？')) return;
      try {
        const res = await api('/api/cluster/gdrive/disconnect', { method: 'POST' });
        if (res.ok) {
          showToast('已断开 Google Drive 连接');
          checkGDriveStatus();
          loadTopology();
        }
      } catch (e) {
        showToast('断开失败: ' + (e.error || e.message));
      }
    }

    // Reactive SSE: Targeted DOM updates (Zero whole-table wipes -> Zero jitter)
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
      const parent = p.includes('/') ? p.substring(0, p.lastIndexOf('/')) : '';
      dirCache.delete(parent);
      dirCache.delete(p);
      const cur = currentPath ? currentPath + '/' : '';
      if (!p.startsWith(cur)) return;
      const sub = p.substring(cur.length);
      if (sub.includes('/')) return;

      const rowId = `row-${escapeId(p)}`;
      const existingRow = document.getElementById(rowId);

      if (kind === 'deleted') {
        if (existingRow) {
          existingRow.remove();
          const idx = currentEntries.findIndex(e => e.name === sub);
          if (idx !== -1) currentEntries.splice(idx, 1);
        }
      } else if (kind === 'created') {
        const item = {
          name: sub,
          path: p,
          type: data.type || 'file',
          size: data.size || 0,
          item_count: data.type === 'dir' ? 0 : null,
          mtime: data.ts || new Date().toISOString(),
          sha256: null
        };
        currentEntries.unshift(item);
        if (!existingRow) {
          const tbody = document.getElementById('fileTableBody');
          tbody.insertAdjacentHTML('afterbegin', renderRowHtml(item));
        }
      } else if (kind === 'updated') {
        const item = currentEntries.find(e => e.name === sub);
        if (item) {
          item.size = data.size || item.size;
          item.mtime = data.ts || item.mtime;
          if (existingRow) {
            existingRow.outerHTML = renderRowHtml(item);
          }
        }
      }
    }

    // Load Topology: Main & Follower Graph
    async function loadTopology() {
      try {
        const res = await api('/api/cluster/topology');
        if (!res.ok) return;
        const topo = await res.json();

        renderTopologySVG(topo);

        const cardsList = document.getElementById('topoCardsList');
        const list = [];
        if (topo.control_plane) list.push({ ...topo.control_plane, role: 'Main Control Node' });
        for (const w of (topo.workers || [])) {
          const role = (w.node_type === 'gdrive' || w.id === 'gdrive') ? 'Google Drive Cloud Node' : 'Storage Follower';
          list.push({ ...w, role });
        }

        cardsList.innerHTML = list.map(n => {
          const used = n.disk_total_bytes - n.disk_available_bytes;
          const pct = n.disk_total_bytes > 0 ? ((used / n.disk_total_bytes) * 100).toFixed(0) : 0;
          const isGDrive = n.node_type === 'gdrive' || n.id === 'gdrive';
          const cardBorder = isGDrive ? 'style="border-color: rgba(0, 113, 227, 0.35); background: rgba(0, 113, 227, 0.03);"' : '';
          const badgeStyle = isGDrive ? 'style="background: rgba(0, 113, 227, 0.1); color: var(--accent); border-color: rgba(0, 113, 227, 0.3);"' : '';
          const barColor = isGDrive ? 'var(--accent)' : (n.is_local ? 'var(--accent)' : 'var(--success)');
          return `
            <div class="topo-card" ${cardBorder}>
              <div class="topo-card-head">
                <span class="topo-card-title">
                  <span class="status-dot ${n.healthy ? '' : 'disconnected'}"></span>
                  ${isGDrive ? '☁️ ' : ''}${n.name || n.id}
                </span>
                <span class="topo-card-badge" ${badgeStyle}>${n.role} · ${n.rtt_ms !== null ? n.rtt_ms + 'ms' : 'off'}</span>
              </div>
              <div class="topo-card-meta">
                <span>Used: ${pct}%</span>
                <span>Free: ${formatBytes(n.disk_available_bytes)}</span>
              </div>
              <div class="meter-bar">
                <div class="meter-fill" style="width: ${pct}%; background: ${barColor};"></div>
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
      const count = workers.length;

      const workerPositions = [];
      if (count === 1) {
        workerPositions.push(270);
      } else if (count === 2) {
        workerPositions.push(160, 380);
      } else if (count === 3) {
        workerPositions.push(110, 270, 430);
      } else if (count > 3) {
        const step = 420 / (count - 1);
        for (let i = 0; i < count; i++) {
          workerPositions.push(60 + Math.round(i * step));
        }
      }

      let pathsHtml = '';
      let nodesHtml = '';

      // Main Node
      nodesHtml += `
        <g transform="translate(270, 45)">
          <circle r="24" fill="#FFFFFF" stroke="#0071E3" stroke-width="2.5"/>
          <circle r="4" fill="#34C759"/>
          <text y="-30" text-anchor="middle" fill="#1D1D1F" font-size="11" font-weight="600">${cp ? cp.name : 'Main'}</text>
          <text y="38" text-anchor="middle" fill="#86868B" font-size="9" font-family="monospace">Primary Entry (:8781)</text>
        </g>
      `;

      workers.forEach((w, i) => {
        const x = workerPositions[i] || (150 + i * 110);
        const y = 165;
        const isGDrive = w.node_type === 'gdrive' || w.id === 'gdrive';
        const strokeColor = isGDrive ? '#0071E3' : (w.healthy ? '#34C759' : '#FF3B30');
        const roleLabel = isGDrive ? 'Cloud Node' : 'Follower';
        const displayName = isGDrive ? '☁️ Google Drive' : (w.name || w.id);

        pathsHtml += `
          <path d="M270 65 Q${Math.round((270 + x) / 2)} 115 ${x} ${y - 12}" fill="none" stroke="${strokeColor}" stroke-width="2" stroke-dasharray="4 4" opacity="0.8"/>
        `;

        nodesHtml += `
          <g transform="translate(${x}, ${y})">
            <circle r="20" fill="#FFFFFF" stroke="${strokeColor}" stroke-width="${isGDrive ? '2.5' : '2'}"/>
            <circle r="3" fill="${w.healthy ? '#34C759' : '#FF3B30'}"/>
            <text y="32" text-anchor="middle" fill="#1D1D1F" font-size="11" font-weight="${isGDrive ? '600' : '500'}">${displayName}</text>
            <text y="44" text-anchor="middle" fill="${isGDrive ? 'var(--accent)' : '#86868B'}" font-size="9" font-family="monospace">${w.rtt_ms !== null ? w.rtt_ms + 'ms' : 'off'} · ${roleLabel}</text>
          </g>
        `;
      });

      svg.innerHTML = pathsHtml + nodesHtml;
    }

    function getDirUrl(p) {
      const base = window.location.pathname;
      return p ? `${base}?path=${encodeURIComponent(p)}` : base;
    }

    function navigate(p, addToHistory = true) {
      loadDirectory(p, false, addToHistory);
    }

    function navigateUp() {
      if (!currentPath) return;
      const parts = currentPath.split('/').filter(Boolean);
      parts.pop();
      navigate(parts.join('/'), true);
    }

    function updateNavControls() {
      const backBtn = document.getElementById('btnNavBack');
      if (backBtn) {
        backBtn.style.display = currentPath ? 'inline-flex' : 'none';
      }
      const syncCurBtn = document.getElementById('btnSyncFolder');
      if (syncCurBtn) {
        syncCurBtn.title = `同步当前目录 (/${currentPath || 'root'}) 到指定节点`;
      }
    }

    window.addEventListener('popstate', (e) => {
      let targetPath = '';
      if (e.state && typeof e.state.path === 'string') {
        targetPath = e.state.path;
      } else {
        const params = new URLSearchParams(window.location.search);
        targetPath = params.get('path') || '';
      }
      loadDirectory(targetPath, false, false);
    });

    // Row selection for batch operations
    function toggleSelectAllRows(checked) {
      document.querySelectorAll('.row-select-cb').forEach(cb => {
        cb.checked = checked;
        const p = cb.getAttribute('data-path');
        if (p) {
          if (checked) selectedRowPaths.add(p);
          else selectedRowPaths.delete(p);
        }
      });
      updateBatchBar();
    }

    function onRowSelectionChange(event) {
      if (event) event.stopPropagation();
      document.querySelectorAll('.row-select-cb').forEach(cb => {
        const p = cb.getAttribute('data-path');
        if (p) {
          if (cb.checked) selectedRowPaths.add(p);
          else selectedRowPaths.delete(p);
        }
      });
      updateSelectAllCheckbox();
      updateBatchBar();
    }

    function updateSelectAllCheckbox() {
      const allCbs = document.querySelectorAll('.row-select-cb');
      const selectAll = document.getElementById('selectAllCb');
      if (!selectAll || allCbs.length === 0) return;
      const checkedCount = document.querySelectorAll('.row-select-cb:checked').length;
      selectAll.checked = checkedCount === allCbs.length;
      selectAll.indeterminate = checkedCount > 0 && checkedCount < allCbs.length;
    }

    function clearRowSelection() {
      selectedRowPaths.clear();
      document.querySelectorAll('.row-select-cb').forEach(cb => cb.checked = false);
      const selectAll = document.getElementById('selectAllCb');
      if (selectAll) {
        selectAll.checked = false;
        selectAll.indeterminate = false;
      }
      updateBatchBar();
    }

    function updateBatchBar() {
      const bar = document.getElementById('batchActionBar');
      const lbl = document.getElementById('batchSelectedLabel');
      if (!bar) return;
      if (selectedRowPaths.size > 0) {
        bar.style.display = 'flex';
        if (lbl) lbl.textContent = `已选择 ${selectedRowPaths.size} 项`;
      } else {
        bar.style.display = 'none';
      }
    }

    // Object Explorer: True Lazy Load with SWR Client-Side Cache
    async function loadDirectory(path = currentPath, append = false, addToHistory = true) {
      if (isLoading) return;
      isLoading = true;

      if (!append) {
        currentPath = path;
        currentOffset = 0;
        clearRowSelection();
        updateBreadcrumbs();
        updateNavControls();

        if (addToHistory) {
          try {
            const url = getDirUrl(currentPath);
            window.history.pushState({ path: currentPath }, '', url);
          } catch (e) {
            console.warn('history.pushState failed:', e);
          }
        }
        try {
          sessionStorage.setItem('ostore_current_path', currentPath);
        } catch (e) {}

        const cached = dirCache.get(path);
        if (cached && (Date.now() - cached.timestamp < DIR_CACHE_TTL_MS)) {
          currentEntries = cached.entries;
          currentOffset = cached.offset;
          currentTotalCount = cached.totalCount;
          currentTotalBytes = cached.totalBytes;
          hasMore = cached.hasMore;
          document.getElementById('folderStatBadge').textContent = 
            `${currentTotalCount.toLocaleString()} items · Total ${formatBytes(currentTotalBytes)}`;
          renderTable(currentEntries, false);
          probeDirectoryReplicas(path);
          if (Date.now() - cached.timestamp < 10000) {
            isLoading = false;
            return;
          }
        } else {
          currentEntries = [];
          document.getElementById('fileTableBody').innerHTML = `
            <tr><td colspan="6" style="text-align:center; padding: 24px; color: var(--ink-secondary); font-family:var(--font-mono);">
              ⚡ Reading directory metadata...
            </td></tr>
          `;
        }
      }

      try {
        let query = `?limit=${PAGE_LIMIT}&offset=${currentOffset}`;
        if (currentPath) query += `&path=${encodeURIComponent(currentPath)}`;

        const res = await api(`/api/list${query}`);
        if (!res.ok) {
          return;
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
        probeDirectoryReplicas(currentPath);

        if (!append) {
          dirCache.set(currentPath, {
            entries: currentEntries,
            totalCount: currentTotalCount,
            totalBytes: currentTotalBytes,
            hasMore,
            offset: currentOffset,
            timestamp: Date.now(),
          });
        }

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
        if (currentPath && !q) {
          const parentRowHtml = `
            <tr class="file-row parent-dir-row" onclick="navigateUp()" style="cursor: pointer; background: #FAFBFD;">
              <td style="text-align: center;"></td>
              <td colspan="4">
                <div class="name-cell" style="color: var(--accent); font-weight: 600;">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="16" height="16"><polyline points="15 18 9 12 15 6"></polyline></svg>
                  <span>.. (返回上一级目录)</span>
                </div>
              </td>
              <td style="text-align: right;">
                <button class="btn btn-sm" onclick="event.stopPropagation(); navigateUp()" style="padding: 2px 10px; font-size: 0.72rem;">返回上一级</button>
              </td>
            </tr>
          `;
          tbody.insertAdjacentHTML('beforeend', parentRowHtml);
        }
      }

      if (filtered.length === 0 && !append) {
        if (!currentPath || q) {
          tbody.innerHTML = `<tr><td colspan="6" style="text-align:center; padding: 32px; color: var(--ink-secondary);">Folder is empty</td></tr>`;
        }
        return;
      }

      const rowsHtml = (append ? filtered.slice(filtered.length - PAGE_LIMIT) : filtered)
        .map(item => renderRowHtml(item))
        .join('');

      tbody.insertAdjacentHTML('beforeend', rowsHtml);
      updateSelectAllCheckbox();
    }

    function renderRowHtml(item) {
      const isDir = item.type === 'dir';
      const dlUrl = isDir ? `${getBasePath()}/api/archive?path=${encodeURIComponent(item.path)}` : `${getBasePath()}/api/files/${encodeURIComponent(item.path)}`;

      let nodeMapHtml = '-';
      if (!isDir) {
        const cached = replicaMapCache.get(item.path);
        const hasCc2 = cached ? cached.cc2 : null;
        const hasRn = cached ? cached.rn : null;
        const hasGd = cached ? cached.gd : null;
        const isProbing = hasCc2 === null || hasRn === null;

        const cc2DotClass = hasCc2 === null ? 'probing' : (hasCc2 === true ? 'present' : 'missing');
        const rnDotClass = hasRn === null ? 'probing' : (hasRn === true ? 'present' : 'missing');
        const gdDotClass = hasGd === null ? 'probing' : (hasGd === true ? 'present' : 'missing');
        const count = 1 + (hasCc2 === true ? 1 : 0) + (hasRn === true ? 1 : 0) + (hasGd === true ? 1 : 0);
        const ratioClass = isProbing ? 'probing' : (count === 4 ? 'full' : 'partial');
        const ratioText = isProbing ? '...' : `${count}/4`;

        nodeMapHtml = `
          <div class="node-badges-cluster" id="badge-${escapeId(item.path)}" onclick="inspectFileReplica('${escapeJs(item.path)}')" title="查看副本拓扑详情与单文件同步">
            <span class="node-dot-item present"><span class="mini-dot main"></span>cc</span>
            <span class="node-dot-item ${hasCc2 === true ? 'present' : ''}"><span class="mini-dot ${cc2DotClass}"></span>cc2</span>
            <span class="node-dot-item ${hasRn === true ? 'present' : ''}"><span class="mini-dot ${rnDotClass}"></span>rn</span>
            <span class="node-dot-item ${hasGd === true ? 'present' : ''}"><span class="mini-dot ${gdDotClass}"></span>gd</span>
            <span class="ratio-badge ${ratioClass}">${ratioText}</span>
          </div>
        `;
      }

      let sizeDisplayHtml = '';
      if (isDir) {
        const itemCount = item.item_count !== undefined && item.item_count !== null ? `${item.item_count.toLocaleString()} items` : 'Directory';
        sizeDisplayHtml = `
          <div class="dir-size-cell" onclick="openFolderDetail('${escapeJs(item.path)}', '${escapeJs(item.name)}', ${item.size || 0}, ${item.item_count || 0}, '${escapeJs(item.mtime || '')}')" title="Click to inspect folder properties">
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
            <button class="icon-btn" title="同步此文件夹到指定节点" onclick="openItemSync('${escapeJs(item.path)}', true, event)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
            </button>
            <a class="icon-btn" href="${dlUrl}" download="${escapeHtml(item.name)}.tar" title="Download folder as tar archive">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
            </a>
            <button class="icon-btn" title="Folder properties" onclick="openFolderDetail('${escapeJs(item.path)}', '${escapeJs(item.name)}', ${item.size || 0}, ${item.item_count || 0}, '${escapeJs(item.mtime || '')}')">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
            </button>
            <button class="icon-btn" title="Rename folder" onclick="renamePrompt('${escapeJs(item.path)}')">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
            </button>
            <button class="icon-btn btn-delete" title="Delete folder" onclick="deleteItem('${escapeJs(item.path)}', true)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
            </button>
          </div>
        `;
      } else {
        actionsHtml = `
          <div class="row-actions">
            <button class="icon-btn" title="同步此文件到指定节点" onclick="openItemSync('${escapeJs(item.path)}', false, event)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
            </button>
            <a class="icon-btn" href="${dlUrl}?download=1" download="${escapeHtml(item.name)}" title="Download">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
            </a>
            <button class="icon-btn" title="Copy Direct URL" onclick="copyURL('${dlUrl}')">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
            </button>
            <button class="icon-btn" title="Copy MD5" onclick="copyMD5('${escapeJs(item.md5 || item.sha256 || '')}')">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
            <button class="icon-btn btn-delete" title="Delete" onclick="deleteItem('${escapeJs(item.path)}', false)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
            </button>
          </div>
        `;
      }

      const isChecked = selectedRowPaths.has(item.path);

      return `
        <tr class="file-row" id="row-${escapeId(item.path)}">
          <td style="text-align: center;">
            <input type="checkbox" class="row-select-cb" data-path="${escapeHtml(item.path)}" data-is-dir="${isDir}" ${isChecked ? 'checked' : ''} onchange="onRowSelectionChange(event)" onclick="event.stopPropagation()">
          </td>
          <td>
            <div class="name-cell" onclick="${isDir ? `navigate('${escapeJs(item.path)}')` : `openMediaPreview('${escapeJs(item.path)}', ${item.size || 0}, '${escapeJs(item.sha256 || '')}')`}">
              ${isDir
                ? `<svg viewBox="0 0 24 24" fill="#0071E3"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>`
                : getFileIcon(item.name)
              }
              <span>${escapeHtml(item.name)}</span>
            </div>
          </td>
          <td>${sizeDisplayHtml}</td>
          <td class="meta-mono">${formatDate(item.mtime)}</td>
          <td>${nodeMapHtml}</td>
          <td>${actionsHtml}</td>
        </tr>
      `;
    }

    function onFilter() {
      renderTable(currentEntries, false);
    }

    function escapeId(str) {
      return btoa(unescape(encodeURIComponent(str))).replace(/[/+=]/g, '_');
    }

    // Infinite Scroll Observer for Streaming Pagination
    const scrollObserver = new IntersectionObserver(entries => {
      if (entries[0].isIntersecting && hasMore && !isLoading) {
        loadDirectory(currentPath, true);
      }
    }, { root: document.getElementById('tableViewport'), threshold: 0.1 });

    scrollObserver.observe(document.getElementById('scrollAnchor'));

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
      const target = inspectingFolderPath;
      closeFolderDetailModal();
      openItemSync(target, true);
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
        let hasGd = false;

        let html = `
          <div class="file-node-row">
            <div>
              <div style="font-weight:600; font-size: 0.84rem; display: flex; align-items: center; gap: 6px;">
                <span class="status-dot ${mainNode.exists ? '' : 'disconnected'}"></span>
                <span>${mainNode.name}</span>
                <span class="topo-card-badge">Main Node (Control Plane)</span>
              </div>
              <div style="font-size: 0.72rem; font-family: var(--font-mono); color: var(--ink-secondary); margin-top: 4px;">
                ${(mainNode.md5 || mainNode.sha256) ? `MD5: ${(mainNode.md5 || mainNode.sha256).substring(0, 16)}...` : 'Status: Present on Main Local Disk'}
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
          if ((f.id === 'gdrive' || (f.name && f.name.includes('Google Drive'))) && isMatched) hasGd = true;

          const actionBtn = isMatched ? '' : `
            <button class="btn btn-primary" onclick="syncFileToNode('${escapeJs(filePath)}', '${escapeJs(f.id)}')" style="margin-top: 6px; padding: 2px 8px; font-size: 0.72rem; align-self: flex-end;">
              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
              同步到此节点
            </button>
          `;

          html += `
            <div class="file-node-row">
              <div>
                <div style="font-weight:600; font-size: 0.84rem; display: flex; align-items: center; gap: 6px;">
                  <span class="status-dot ${isMatched ? '' : 'disconnected'}"></span>
                  <span>${escapeHtml(f.name)}</span>
                  <span class="topo-card-badge">Follower · ${f.rtt_ms !== null ? f.rtt_ms + 'ms' : 'off'}</span>
                </div>
                <div style="font-size: 0.72rem; font-family: var(--font-mono); color: var(--ink-secondary); margin-top: 4px;">
                  ${f.exists 
                    ? ((f.md5 || f.sha256) ? `MD5: ${(f.md5 || f.sha256).substring(0, 16)}...` : 'Matched Size & ETag') 
                    : '<strong style="color:var(--danger)">Missing on this node</strong>'}
                </div>
              </div>
              <div style="display: flex; flex-direction: column; align-items: flex-end; text-align: right; font-family: var(--font-mono); font-size: 0.78rem;">
                <div>${f.exists ? formatBytes(f.size) : '0 B'}</div>
                <span class="ratio-badge ${isMatched ? 'full' : 'partial'}" style="font-size:0.7rem;">
                  ${isMatched ? 'Replica Synced' : (f.exists ? 'Size Mismatch' : 'Not Replicated')}
                </span>
                ${actionBtn}
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

        replicaMapCache.set(filePath, { cc2: hasCc2, rn: hasRn, gd: hasGd });
        updateRowBadge(filePath, hasCc2, hasRn, hasGd);

        document.getElementById('fileTopoSummary').textContent = 
          `Status: ${data.replica_count}/${data.expected_replicas} Nodes Hold Replicas`;

      } catch (e) {
        document.getElementById('fileTopoFlow').innerHTML = `<div style="color: var(--danger); padding: 14px;">Error querying replica: ${e}</div>`;
      }
    }

    async function syncFileToNode(filePath, nodeId) {
      showToast(`正在同步 ${filePath.split('/').pop()} 到 ${nodeId}...`);
      try {
        const res = await api('/api/cluster/sync', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ paths: [filePath], target_nodes: [nodeId] }),
        });
        if (res.ok) {
          showToast(`已成功同步到 ${nodeId}!`);
          inspectFileReplica(filePath, true);
          probeDirectoryReplicas(currentPath);
        } else {
          showToast(`同步到 ${nodeId} 失败: HTTP ${res.status}`);
        }
      } catch (e) {
        showToast(`同步异常: ${e}`);
      }
    }

    function updateRowBadge(filePath, hasCc2, hasRn, hasGd) {
      const badge = document.getElementById(`badge-${escapeId(filePath)}`);
      if (!badge) return;
      const isProbing = hasCc2 === null || hasRn === null;
      const cc2DotClass = hasCc2 === null ? 'probing' : (hasCc2 === true ? 'present' : 'missing');
      const rnDotClass = hasRn === null ? 'probing' : (hasRn === true ? 'present' : 'missing');
      const gdDotClass = hasGd === null ? 'probing' : (hasGd === true ? 'present' : 'missing');
      const count = 1 + (hasCc2 === true ? 1 : 0) + (hasRn === true ? 1 : 0) + (hasGd === true ? 1 : 0);
      const ratioClass = isProbing ? 'probing' : (count === 4 ? 'full' : 'partial');
      const ratioText = isProbing ? '...' : `${count}/4`;

      badge.innerHTML = `
        <span class="node-dot-item present"><span class="mini-dot main"></span>cc</span>
        <span class="node-dot-item ${hasCc2 === true ? 'present' : ''}"><span class="mini-dot ${cc2DotClass}"></span>cc2</span>
        <span class="node-dot-item ${hasRn === true ? 'present' : ''}"><span class="mini-dot ${rnDotClass}"></span>rn</span>
        <span class="node-dot-item ${hasGd === true ? 'present' : ''}"><span class="mini-dot ${gdDotClass}"></span>gd</span>
        <span class="ratio-badge ${ratioClass}">${ratioText}</span>
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
        probeDirectoryReplicas(currentPath);
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

    async function openMediaPreview(path, size = 0, sha256 = '') {
      const filename = path.split('/').pop();
      const fileInfo = getFileCategory(filename);
      const url = `${getBasePath()}/api/files/${path}`;
      currentPreviewUrl = url;
      currentPreviewText = '';

      const modal = document.getElementById('mediaModal');
      const container = document.getElementById('mediaContainer');
      const metricsBar = document.getElementById('mediaMetricsBar');
      const encodingTag = document.getElementById('mediaEncodingTag');
      const iconSpan = document.getElementById('mediaIcon');
      const downloadBtn = document.getElementById('mediaDownloadBtn');
      const copyBtn = document.getElementById('mediaCopyBtn');

      document.getElementById('mediaTitle').textContent = filename;
      encodingTag.textContent = fileInfo.tag;
      if (iconSpan) iconSpan.innerHTML = getFileIcon(filename);
      metricsBar.innerHTML = '<span>⚡ Loading object & metrics...</span><span>--</span>';
      container.innerHTML = '<div style="color:var(--ink-secondary); padding: 40px; font-family:var(--font-mono); font-size:0.82rem;">Loading stream...</div>';

      if (downloadBtn) {
        downloadBtn.href = url;
        downloadBtn.setAttribute('download', filename);
      }
      if (copyBtn) copyBtn.style.display = 'none';

      modal.style.display = 'flex';

      const t0 = performance.now();

      // 1. Direct Image
      if (fileInfo.category === 'image') {
        container.innerHTML = `<img src="${url}" style="max-width: 100%; max-height: 68vh; border-radius: 8px; object-fit: contain; align-self: center;">`;
        const dur = (performance.now() - t0).toFixed(1);
        metricsBar.innerHTML = `<span>⚡ Image View · Client Render: ${dur}ms</span><span>HTTP Stream</span>`;
        return;
      }

      // 2. Direct Video
      if (fileInfo.category === 'video') {
        container.innerHTML = `<video src="${url}" controls autoplay style="max-width: 100%; max-height: 68vh; border-radius: 8px; align-self: center;"></video>`;
        metricsBar.innerHTML = `<span>⚡ HTML5 Media Stream · Range Enabled</span><span>HTTP 206 Partial</span>`;
        return;
      }

      // 3. Direct Audio
      if (fileInfo.category === 'audio') {
        container.innerHTML = `
          <div style="display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 40px 20px; width: 100%; align-self: center;">
            <div style="width: 44px; height: 44px; margin-bottom: 20px; color: var(--accent);">${getFileIcon(filename)}</div>
            <audio src="${url}" controls autoplay style="width: 100%; max-width: 460px;"></audio>
          </div>
        `;
        metricsBar.innerHTML = `<span>⚡ HTML5 Audio Stream</span><span>HTTP Stream</span>`;
        return;
      }

      // 4. PDF Document
      if (fileInfo.category === 'pdf') {
        container.innerHTML = `<iframe src="${url}" style="width: 80vw; height: 68vh; border: 1px solid var(--card-border); border-radius: 8px;"></iframe>`;
        metricsBar.innerHTML = `<span>⚡ Native PDF Preview</span><span>Embedded View</span>`;
        return;
      }

      // 5. Large Known Binary / Archive (> 3MB): Show Info Card with Download actions instead of fetching
      if ((fileInfo.category === 'archive' || fileInfo.category === 'binary') && size > 3 * 1024 * 1024) {
        container.innerHTML = `
          <div style="display: flex; flex-direction: column; align-items: center; text-align: center; padding: 36px 20px; max-width: 520px; align-self: center; width: 100%;">
            <div style="width: 48px; height: 48px; margin-bottom: 16px;">${getFileIcon(filename)}</div>
            <div style="font-weight: 600; font-size: 1rem; margin-bottom: 6px;">${escapeHtml(filename)}</div>
            <div style="font-family: var(--font-mono); font-size: 0.8rem; color: var(--ink-secondary); margin-bottom: 18px;">
              ${fileInfo.tag} Object · ${formatBytes(size)}
            </div>
            <div style="font-size: 0.82rem; color: var(--ink-secondary); line-height: 1.5; margin-bottom: 24px;">
              Direct inline preview is not supported for ${fileInfo.tag} binary objects. You can download the file or copy its link.
            </div>
            <div style="display: flex; gap: 10px;">
              <a href="${url}" download="${escapeHtml(filename)}" class="btn-primary" style="text-decoration:none; display:inline-flex; align-items:center; gap:6px;">
                Download Object (${formatBytes(size)})
              </a>
              <button class="btn-secondary" onclick="copyURL('${url}')">Copy Link</button>
            </div>
          </div>
        `;
        metricsBar.innerHTML = `<span>⚡ Binary Object Metadata</span><span>Size: <strong>${formatBytes(size)}</strong></span>`;
        return;
      }

      // 6. Text / Code / Config / Unknown file: Fetch & Smart Sniff / Decode
      try {
        const resp = await api(`/api/files/${path}`);
        if (!resp.ok) {
          throw new Error(`HTTP ${resp.status} ${resp.statusText}`);
        }

        const serverTiming = resp.headers.get('Server-Timing') || 'local';
        const contentType = resp.headers.get('Content-Type') || '';
        const buf = await resp.arrayBuffer();
        const tNet = (performance.now() - t0).toFixed(1);

        // Check if server headers revealed an image/video/audio format
        if (contentType.startsWith('image/') && !contentType.includes('svg')) {
          container.innerHTML = `<img src="${url}" style="max-width: 100%; max-height: 68vh; border-radius: 8px; object-fit: contain; align-self: center;">`;
          metricsBar.innerHTML = `<span>⚡ Image View · Network: ${tNet}ms</span><span>HTTP Stream</span>`;
          return;
        }

        const uint8 = new Uint8Array(buf);
        const decoded = decodeTextSmart(uint8);

        encodingTag.textContent = decoded.encoding;

        if (decoded.isBinary) {
          container.innerHTML = `
            <div style="width:100%;">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <div style="color:var(--warning); font-size:0.78rem; font-family:var(--font-mono);">
                  ⚠️ Binary / Encrypted Data Detected (${decoded.encoding}). Displaying Hex Dump:
                </div>
                <a href="${url}" download="${escapeHtml(filename)}" class="btn-secondary" style="font-size:0.72rem; padding:3px 10px; text-decoration:none;">
                  Download Raw Binary
                </a>
              </div>
              <pre>${escapeHtml(decoded.text)}</pre>
            </div>
          `;
        } else {
          currentPreviewText = decoded.text;
          if (copyBtn) copyBtn.style.display = 'inline-flex';

          let displayText = decoded.text;
          const isTruncated = displayText.length > 350000;
          if (isTruncated) {
            displayText = displayText.substring(0, 350000) + '\n\n... [Content truncated for preview performance. Download full file to view complete content]';
          }

          container.innerHTML = `<pre style="tab-size: 4; width: 100%;">${escapeHtml(displayText)}</pre>`;
        }

        metricsBar.innerHTML = `
          <span>⚡ Server Time: <strong>${serverTiming}</strong> · Network RTT: <strong>${tNet}ms</strong></span>
          <span>Size: <strong>${formatBytes(buf.byteLength)}</strong></span>
        `;
      } catch (e) {
        container.innerHTML = `
          <div style="padding: 30px; text-align: center; width: 100%; align-self: center;">
            <div style="color: var(--danger); font-size: 0.9rem; margin-bottom: 12px;">Failed to load object preview: ${escapeHtml(String(e))}</div>
            <a href="${url}" download="${escapeHtml(filename)}" class="btn-secondary" style="text-decoration:none; display:inline-block;">Try Direct Download</a>
          </div>
        `;
        metricsBar.innerHTML = `<span>⚠️ Error loading stream</span><span>--</span>`;
      }
    }

    function closeMediaModal() {
      currentPreviewText = '';
      currentPreviewUrl = '';
      const container = document.getElementById('mediaContainer');
      if (container) container.innerHTML = '';
      const modal = document.getElementById('mediaModal');
      if (modal) modal.style.display = 'none';
      const copyBtn = document.getElementById('mediaCopyBtn');
      if (copyBtn) copyBtn.style.display = 'none';
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

    function copyMD5(md5) {
      if (!md5) return;
      navigator.clipboard.writeText(md5);
      showToast('MD5 checksum copied to clipboard');
    }
    const copySHA = copyMD5;

    async function deleteItem(path, isDir) {
      if (!confirm(`Delete ${isDir ? 'folder' : 'file'} "${path}"?`)) return;
      const res = await api(`/api/files/${path}${isDir ? '?recursive=1' : ''}`, { method: 'DELETE' });
      if (res.ok) {
        dirCache.delete(currentPath);
        dirCache.delete(path);
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
        dirCache.delete(currentPath);
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
      dirCache.delete(currentPath);
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
      dirCache.delete(currentPath);
      showToast('Folder tree uploaded successfully');
      e.target.value = '';
      loadDirectory(currentPath, false);
    }

    function timeAgo(date) {
      if (!date) return '';
      const seconds = Math.floor((new Date() - new Date(date)) / 1000);
      if (seconds < 5) return 'just now';
      if (seconds < 60) return `${seconds}s ago`;
      const minutes = Math.floor(seconds / 60);
      if (minutes < 60) return `${minutes}m ago`;
      const hours = Math.floor(minutes / 60);
      if (hours < 24) return `${hours}h ago`;
      const days = Math.floor(hours / 24);
      return `${days}d ago`;
    }

    async function probeDirectoryReplicas(dirPath) {
      try {
        const res = await api(`/api/cluster/dir-status?prefix=${encodeURIComponent(dirPath)}`);
        if (!res.ok) return;
        const data = await res.json();
        if (!data.files) return;
        for (const [filePath, info] of Object.entries(data.files)) {
          replicaMapCache.set(filePath, { cc2: info.cc2, rn: info.rn, gd: info.gd });
          updateRowBadge(filePath, info.cc2, info.rn, info.gd);
        }
      } catch (e) {
        console.warn('Directory replica probe error:', e);
      }
    }

    let syncIntervalTimer = null;
    let syncStartTime = 0;
    let selectedSyncNodes = new Set();
    let cachedDiffList = [];

    function onSyncScopeChanged() {
      const panel = document.getElementById('syncDiffPanel');
      if (panel) panel.style.display = 'none';
      cachedDiffList = [];
      const runBtnText = document.getElementById('btnRunSyncText');
      if (runBtnText) runBtnText.textContent = 'Run Sync Now';
    }

    function toggleNodeSelect(nodeId) {
      if (selectedSyncNodes.has(nodeId)) {
        selectedSyncNodes.delete(nodeId);
      } else {
        selectedSyncNodes.add(nodeId);
      }
      document.querySelectorAll('.fleet-node-item').forEach(el => {
        const id = el.getAttribute('data-node-id');
        if (id && selectedSyncNodes.has(id)) {
          el.classList.add('selected');
        } else if (id) {
          el.classList.remove('selected');
        }
      });
      refreshSyncStatus();
    }

    function openItemSync(targetPath, isDir, event) {
      if (event) event.stopPropagation();
      activeSyncTarget = {
        type: isDir ? 'dir' : 'file',
        paths: isDir ? [] : [targetPath],
        prefix: isDir ? targetPath : '',
        label: `${isDir ? '文件夹' : '文件'}: ${targetPath.split('/').pop()}`,
      };
      setupSyncModalWithTarget();
    }

    function openBatchSync() {
      const selected = Array.from(selectedRowPaths);
      if (selected.length === 0) {
        showToast('请先选择要同步的文件或文件夹');
        return;
      }
      activeSyncTarget = {
        type: 'batch',
        paths: selected,
        prefix: '',
        label: `已选中 ${selected.length} 个项目`,
      };
      setupSyncModalWithTarget();
    }

    function openCurrentFolderSync() {
      activeSyncTarget = {
        type: 'current',
        paths: [],
        prefix: currentPath,
        label: `当前目录: /${currentPath || ' (root)'}`,
      };
      setupSyncModalWithTarget();
    }

    function setupSyncModalWithTarget() {
      const itemWrapper = document.getElementById('syncItemScopeWrapper');
      const itemLabel = document.getElementById('syncItemScopeLabel');
      const badge = document.getElementById('syncTargetBadge');
      const scopeItemRadio = document.getElementById('syncScopeItem');
      const scopeCurrentRadio = document.getElementById('syncScopeCurrent');
      const scopeAllRadio = document.getElementById('syncScopeAll');

      if (activeSyncTarget && (activeSyncTarget.type === 'file' || activeSyncTarget.type === 'dir' || activeSyncTarget.type === 'batch')) {
        itemWrapper.style.display = 'inline-flex';
        itemLabel.textContent = activeSyncTarget.label;
        scopeItemRadio.checked = true;
        badge.textContent = `🎯 同步目标: ${activeSyncTarget.label} (${activeSyncTarget.type === 'batch' ? activeSyncTarget.paths.length + ' 项' : (activeSyncTarget.prefix || activeSyncTarget.paths[0])})`;
        badge.style.display = 'block';
      } else {
        itemWrapper.style.display = 'none';
        badge.style.display = 'none';
        if (activeSyncTarget && activeSyncTarget.type === 'current') {
          scopeCurrentRadio.checked = true;
        } else {
          scopeAllRadio.checked = true;
        }
      }
      openClusterSyncModal();
    }

    async function openClusterSyncModal() {
      const folderLabel = document.getElementById('syncCurrentFolderLabel');
      if (folderLabel) {
        folderLabel.textContent = `Current Folder (${currentPath ? '/' + currentPath : '/ (root)'})`;
      }
      document.getElementById('syncProgressTimer').textContent = '';
      const diffPanel = document.getElementById('syncDiffPanel');
      if (diffPanel) diffPanel.style.display = 'none';
      const liveContainer = document.getElementById('syncLiveProgressContainer');
      if (liveContainer) liveContainer.style.display = 'none';
      const cancelBtn = document.getElementById('btnCancelSync');
      if (cancelBtn) cancelBtn.style.display = 'none';

      document.getElementById('clusterSyncModal').style.display = 'flex';
      await refreshSyncStatus();
    }

    function closeClusterSyncModal() {
      if (syncIntervalTimer) {
        clearInterval(syncIntervalTimer);
        syncIntervalTimer = null;
      }
      document.getElementById('clusterSyncModal').style.display = 'none';
    }

    async function refreshSyncStatus() {
      try {
        const res = await api('/api/cluster/sync-status');
        if (!res.ok) return;
        const data = await res.json();

        // Render nodes
        const nodesContainer = document.getElementById('syncFleetNodes');
        if (nodesContainer && data.nodes) {
          if (selectedSyncNodes.size === 0) {
            data.nodes.filter(n => !n.is_local).forEach(n => selectedSyncNodes.add(n.id));
          }
          nodesContainer.innerHTML = data.nodes.map(n => `
            <div class="fleet-node-item ${selectedSyncNodes.has(n.id) ? 'selected' : ''}" data-node-id="${escapeHtml(n.id)}" onclick="${n.is_local ? '' : `toggleNodeSelect('${n.id}')`}">
              <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 4px;">
                <span style="font-weight: 600; font-size: 0.82rem; color: var(--ink);">${escapeHtml(n.name)}</span>
                <span class="status-dot ${n.healthy ? '' : 'disconnected'}"></span>
              </div>
              <div style="color: var(--ink-secondary); font-size: 0.72rem; font-family: var(--font-mono);">
                ${n.is_local ? 'Main (Leader)' : `${selectedSyncNodes.has(n.id) ? '✓ Target' : '○ Excluded'} · ${n.rtt_ms !== null ? n.rtt_ms + 'ms' : 'offline'}`}
              </div>
            </div>
          `).join('');
        }

        // Render status & report
        const badge = document.getElementById('syncStatusBadge');
        const report = document.getElementById('syncReportContent');
        const runBtn = document.getElementById('btnRunSync');
        const runBtnText = document.getElementById('btnRunSyncText');
        const cancelBtn = document.getElementById('btnCancelSync');
        const liveContainer = document.getElementById('syncLiveProgressContainer');

        if (data.is_syncing) {
          if (badge) {
            badge.className = 'ratio-badge partial';
            badge.textContent = 'Syncing...';
          }
          if (cancelBtn) cancelBtn.style.display = 'inline-flex';
          if (runBtn) runBtn.disabled = true;
          if (runBtnText) runBtnText.textContent = 'Replicating...';

          if (liveContainer && data.progress) {
            liveContainer.style.display = 'block';
            const p = data.progress;
            document.getElementById('syncLiveProgressBar').style.width = `${Math.min(100, Math.max(0, p.percent || 0))}%`;
            document.getElementById('syncLivePercentText').textContent = `${(p.percent || 0).toFixed(1)}%`;
            document.getElementById('syncLiveStatusText').textContent = `Replicating ${p.synced_count} of ${p.total_to_sync}...`;
            document.getElementById('syncLiveSubText').textContent = p.current_file ? `${p.current_file} -> [${p.current_node}]` : '';
          }
        } else {
          if (cancelBtn) cancelBtn.style.display = 'none';
          if (runBtn) runBtn.disabled = false;
          if (runBtnText && !syncIntervalTimer) {
            runBtnText.textContent = cachedDiffList.length > 0 ? `Sync Selected (${cachedDiffList.length})` : 'Run Sync Now';
          }
          if (liveContainer && !syncIntervalTimer) liveContainer.style.display = 'none';

          const last = data.last_sync;
          if (last) {
            const hasUpdates = (last.total_synced || 0) > 0;
            if (badge) {
              badge.className = 'ratio-badge full';
              badge.textContent = hasUpdates ? 'Updated' : 'Fleet in Sync';
            }
            const nodesHtml = (last.node_results || []).map(nr => `
              <div style="display:flex; justify-content:space-between; margin-top:2px;">
                <span>• Node <strong>${escapeHtml(nr.node_id)}</strong>:</span>
                <span>${nr.synced} synced, ${nr.skipped} in-sync${nr.errors && nr.errors.length ? ` (<span style="color:var(--danger);">${nr.errors.length} err</span>)` : ''}</span>
              </div>
            `).join('');

            if (report) {
              report.innerHTML = `
                <div style="display:flex; justify-content:space-between; margin-bottom: 4px;">
                  <span>Last Sync: <strong>${formatDate(last.timestamp)}</strong></span>
                  <span>${timeAgo(last.timestamp)}</span>
                </div>
                <div style="margin-bottom: 4px;">
                  Scope: <code>${last.prefix ? '/' + escapeHtml(last.prefix) : '/ (entire fleet)'}</code> · Took: <strong>${last.duration_ms}ms</strong>
                </div>
                <div style="margin-bottom: 6px;">
                  Checked: <strong>${(last.total_checked || 0).toLocaleString()}</strong> · Synced: <strong style="color:var(--accent);">${last.total_synced || 0}</strong> · In-sync: <strong>${(last.total_skipped || 0).toLocaleString()}</strong>
                </div>
                <div style="padding-top: 6px; border-top: 1px dashed var(--card-border);">
                  ${nodesHtml}
                </div>
              `;
            }
          } else {
            if (badge) {
              badge.className = 'ratio-badge full';
              badge.textContent = 'Ready';
            }
            if (report) {
              report.innerHTML = `
                <div>No manual sync triggered since daemon startup.</div>
                <div style="margin-top: 4px; color: var(--ink-tertiary);">Objects are automatically synced to followers on upload. Click below to verify or force replication.</div>
              `;
            }
          }
        }
      } catch (e) {
        console.error('Failed to load sync status:', e);
      }
    }

    async function scanSyncDiff() {
      const scopeRadio = document.querySelector('input[name="syncScope"]:checked');
      const scopeVal = scopeRadio ? scopeRadio.value : 'all';
      let prefix = '';
      let paths = undefined;

      if (scopeVal === 'item' && activeSyncTarget) {
        if (activeSyncTarget.type === 'dir') {
          prefix = activeSyncTarget.prefix;
        } else {
          paths = activeSyncTarget.paths;
        }
      } else if (scopeVal === 'current') {
        prefix = currentPath;
      }

      const targetNodes = Array.from(selectedSyncNodes);

      const btn = document.getElementById('btnScanDiff');
      if (btn) btn.disabled = true;
      showToast('Scanning fleet differences...');

      try {
        const payload = {
          prefix: prefix || undefined,
          paths: paths && paths.length > 0 ? paths : undefined,
          target_nodes: targetNodes.length > 0 ? targetNodes : undefined,
        };
        const res = await api('/api/cluster/sync-diff', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();

        const diffPanel = document.getElementById('syncDiffPanel');
        const diffList = document.getElementById('syncDiffList');
        const countSpan = document.getElementById('syncDiffCount');
        const runBtnText = document.getElementById('btnRunSyncText');

        cachedDiffList = data.differences || [];
        countSpan.textContent = cachedDiffList.length;

        if (cachedDiffList.length === 0) {
          diffPanel.style.display = 'block';
          diffList.innerHTML = `<div style="padding: 12px; text-align: center; color: var(--success); font-weight: 500;">✓ All ${data.total_scanned} files are fully synchronized across selected nodes!</div>`;
          showToast('Fleet is fully in sync!');
          if (runBtnText) runBtnText.textContent = 'Run Sync Now';
        } else {
          diffPanel.style.display = 'block';
          diffList.innerHTML = cachedDiffList.map((d, idx) => {
            const badges = [
              ...d.missing_on.map(n => `<span class="diff-badge missing">${n}: Missing</span>`),
              ...d.mismatch_on.map(n => `<span class="diff-badge mismatch">${n}: Mismatch</span>`),
              ...d.in_sync_on.map(n => `<span class="diff-badge ok">${n}: OK</span>`),
            ].join(' ');
            return `
              <div class="sync-diff-row">
                <input type="checkbox" class="diff-file-cb" data-path="${escapeHtml(d.path)}" checked id="diff-cb-${idx}">
                <label for="diff-cb-${idx}" style="cursor:pointer; display:flex; justify-content:space-between; align-items:center; width:100%; min-width:0;">
                  <span style="overflow:hidden; text-overflow:ellipsis; white-space:nowrap; margin-right:8px;" title="${escapeHtml(d.path)}">${escapeHtml(d.path)}</span>
                  <div style="display:flex; align-items:center; gap:6px; flex-shrink:0;">
                    <span style="color:var(--ink-secondary); font-size:0.7rem;">${formatBytes(d.size)}</span>
                    ${badges}
                  </div>
                </label>
              </div>
            `;
          }).join('');
          if (runBtnText) runBtnText.textContent = `Sync Selected (${cachedDiffList.length})`;
          showToast(`Found ${cachedDiffList.length} files needing synchronization`);
        }
      } catch (e) {
        showToast('Scan diff failed: ' + e);
      } finally {
        if (btn) btn.disabled = false;
      }
    }

    function selectAllDiffs(select) {
      document.querySelectorAll('.diff-file-cb').forEach(cb => cb.checked = select);
      const selectedCount = document.querySelectorAll('.diff-file-cb:checked').length;
      const runBtnText = document.getElementById('btnRunSyncText');
      if (runBtnText) runBtnText.textContent = selectedCount > 0 ? `Sync Selected (${selectedCount})` : 'Run Sync Now';
    }

    async function cancelRunningSync() {
      const cancelBtn = document.getElementById('btnCancelSync');
      if (cancelBtn) cancelBtn.disabled = true;
      showToast('Aborting fleet sync...');
      try {
        const res = await api('/api/cluster/sync/cancel', { method: 'POST' });
        if (res.ok) {
          showToast('Sync cancelled gracefully');
        }
      } catch (e) {
        showToast('Cancel error: ' + e);
      } finally {
        if (cancelBtn) cancelBtn.disabled = false;
      }
    }

    async function runClusterSyncFromModal() {
      const scopeRadio = document.querySelector('input[name="syncScope"]:checked');
      const scopeVal = scopeRadio ? scopeRadio.value : 'all';
      let prefix = '';
      let paths = undefined;

      if (scopeVal === 'item' && activeSyncTarget) {
        if (activeSyncTarget.type === 'dir') {
          prefix = activeSyncTarget.prefix;
        } else {
          paths = activeSyncTarget.paths;
        }
      } else if (scopeVal === 'current') {
        prefix = currentPath;
      }

      const targetNodes = Array.from(selectedSyncNodes);

      const checkedFiles = Array.from(document.querySelectorAll('.diff-file-cb:checked')).map(cb => cb.getAttribute('data-path'));
      const hasSpecificFiles = document.getElementById('syncDiffPanel') && document.getElementById('syncDiffPanel').style.display !== 'none' && checkedFiles.length > 0;
      if (hasSpecificFiles) {
        paths = checkedFiles;
        prefix = '';
      }

      const runBtn = document.getElementById('btnRunSync');
      const runBtnText = document.getElementById('btnRunSyncText');
      const cancelBtn = document.getElementById('btnCancelSync');
      const liveContainer = document.getElementById('syncLiveProgressContainer');

      if (runBtn) runBtn.disabled = true;
      if (runBtnText) runBtnText.textContent = 'Syncing...';
      if (cancelBtn) cancelBtn.style.display = 'inline-flex';
      if (liveContainer) liveContainer.style.display = 'block';

      syncStartTime = Date.now();
      syncIntervalTimer = setInterval(async () => {
        await refreshSyncStatus();
      }, 400);

      const payload = {
        prefix: prefix || undefined,
        paths: paths && paths.length > 0 ? paths : undefined,
        target_nodes: targetNodes.length > 0 ? targetNodes : undefined,
        concurrency: 3,
      };

      try {
        const targetDesc = paths ? `${paths.length} files` : (prefix ? `/${prefix}` : 'entire storage');
        showToast(`Starting fleet sync (${targetDesc})...`);
        const res = await api('/api/cluster/sync', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        });
        if (res.ok) {
          const data = await res.json();
          const synced = data.total_synced !== undefined ? data.total_synced : (data.node_results || []).reduce((a, n) => a + (n.synced || 0), 0);
          showToast(`Sync finished: ${synced} files updated (${data.duration_ms}ms)`);
          const diffPanel = document.getElementById('syncDiffPanel');
          if (diffPanel) diffPanel.style.display = 'none';
          cachedDiffList = [];
          await refreshSyncStatus();
          loadDirectory(currentPath, false);
          loadTopology();
        } else {
          showToast('Sync failed: HTTP ' + res.status);
        }
      } catch (e) {
        showToast('Sync error: ' + e);
      } finally {
        if (syncIntervalTimer) {
          clearInterval(syncIntervalTimer);
          syncIntervalTimer = null;
        }
        if (runBtn) runBtn.disabled = false;
        if (runBtnText) runBtnText.textContent = 'Run Sync Now';
        if (cancelBtn) cancelBtn.style.display = 'none';
        if (liveContainer) liveContainer.style.display = 'none';
      }
    }

    // Initial Gatekeeper: Verify authentication cleanly before showing console
    async function checkAuthAndBoot() {
      if (!sessionToken) {
        showLogin();
        return;
      }

      try {
        const res = await api('/api/auth/verify');
        if (res.ok) {
          const data = await res.json();
          if (data && data.authenticated) {
            showConsole();
            return;
          }
        }
      } catch (e) {
        console.warn('Auth verify check error:', e);
      }

      // Invalid or expired token: clear stale session and show login screen cleanly (no mainConsole flash)
      sessionToken = '';
      localStorage.removeItem('ostore_sess');
      showLogin();
    }

    checkAuthAndBoot();
  </script>
</body>
</html>
"##;
