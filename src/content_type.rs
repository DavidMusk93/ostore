//! Intelligent MIME / Content-Type detection for files and objects.
//!
//! Handles:
//! - Timestamped / versioned backups (e.g. `Caddyfile.20261102-214513`, `nginx.conf.bak`, `server.py.old`)
//! - Extensionless config and build files (`Caddyfile`, `Dockerfile`, `Makefile`, `hosts`, etc.)
//! - Dotfiles (`.env`, `.gitignore`, `.editorconfig`)
//! - Modern programming and config formats (`.toml`, `.yaml`, `.rs`, `.go`, `.ts`, `.vue`, etc.)
//! - Content sniffing for unknown files (detecting valid UTF-8 text vs binary)

use std::path::Path;

/// Strip backup / timestamp / version / temporary suffix from a file name.
/// E.g.
/// - "Caddyfile.20261102-214513" -> "Caddyfile"
/// - "nginx.conf.bak" -> "nginx.conf"
/// - "config.yaml.20261102" -> "config.yaml"
/// - "server.py.old" -> "server.py"
pub fn strip_backup_suffix(name: &str) -> &str {
    if let Some((base, ext)) = name.rsplit_once('.') {
        if is_backup_or_timestamp_suffix(ext) {
            return base;
        }
    }
    name
}

fn is_backup_or_timestamp_suffix(s: &str) -> bool {
    let s_lower = s.to_ascii_lowercase();
    match s_lower.as_str() {
        "bak" | "backup" | "old" | "orig" | "original" | "save" | "saved" | "tmp" | "temp"
        | "swp" | "dist" | "default" | "example" | "sample" | "template" | "tpl" => return true,
        _ => {}
    }

    // Pure digits (e.g. .1, .2, .20261102)
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    // Timestamp patterns like 20261102-214513 or 20261102_214513
    if s.len() >= 13 && s.len() <= 16 {
        let parts: Vec<&str> = s.split(|c| c == '-' || c == '_').collect();
        if parts.len() == 2
            && parts[0].chars().all(|c| c.is_ascii_digit())
            && parts[1].chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }

    false
}

fn exact_filename_content_type(lower: &str) -> Option<&'static str> {
    match lower {
        "caddyfile" => Some("text/plain; charset=utf-8"),
        "dockerfile" | "containerfile" => Some("text/plain; charset=utf-8"),
        "makefile" | "gnumakefile" | "justfile" | "earthfile" | "taskfile"
        | "procfile" | "gemfile" | "rakefile" | "vagrantfile" | "brewfile" => {
            Some("text/plain; charset=utf-8")
        }
        "hosts" | "resolv.conf" | "crontab" | "sudoers" | "fstab" | "inittab" => {
            Some("text/plain; charset=utf-8")
        }
        "license" | "copying" | "authors" | "changelog" | "changes" | "readme" | "todo"
        | "notice" | "patents" => Some("text/plain; charset=utf-8"),
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".dockerignore" | ".editorconfig"
        | ".npmrc" | ".zshrc" | ".bashrc" | ".profile" | ".tmux.conf" | ".prettierrc"
        | ".eslintrc" | ".babelrc" => Some("text/plain; charset=utf-8"),
        _ => None,
    }
}

fn extension_content_type(ext: &str) -> Option<&'static str> {
    match ext {
        // Config & infra
        "caddyfile" | "caddy" => Some("text/plain; charset=utf-8"),
        "toml" => Some("application/toml; charset=utf-8"),
        "yaml" | "yml" => Some("text/yaml; charset=utf-8"),
        "json" | "json5" | "jsonc" => Some("application/json; charset=utf-8"),
        "jsonl" | "ndjson" => Some("application/x-ndjson; charset=utf-8"),
        "ini" | "conf" | "cnf" | "cfg" | "properties" | "service" | "unit" | "rules" | "rc" => {
            Some("text/plain; charset=utf-8")
        }
        "env" => Some("text/plain; charset=utf-8"),

        // Source code
        "rs" => Some("text/x-rust; charset=utf-8"),
        "go" => Some("text/x-go; charset=utf-8"),
        "py" | "pyw" => Some("text/x-python; charset=utf-8"),
        "js" | "mjs" | "cjs" => Some("text/javascript; charset=utf-8"),
        "ts" => Some("text/typescript; charset=utf-8"),
        "tsx" => Some("text/typescript-jsx; charset=utf-8"),
        "jsx" => Some("text/jsx; charset=utf-8"),
        "c" | "h" => Some("text/x-c; charset=utf-8"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("text/x-c++; charset=utf-8"),
        "java" => Some("text/x-java; charset=utf-8"),
        "kt" | "kts" => Some("text/x-kotlin; charset=utf-8"),
        "scala" => Some("text/x-scala; charset=utf-8"),
        "rb" => Some("text/x-ruby; charset=utf-8"),
        "php" => Some("text/x-php; charset=utf-8"),
        "lua" => Some("text/x-lua; charset=utf-8"),
        "pl" | "pm" => Some("text/x-perl; charset=utf-8"),
        "r" => Some("text/x-r; charset=utf-8"),
        "swift" => Some("text/x-swift; charset=utf-8"),
        "dart" => Some("text/x-dart; charset=utf-8"),
        "cs" => Some("text/x-csharp; charset=utf-8"),
        "fs" => Some("text/x-fsharp; charset=utf-8"),
        "sh" | "bash" | "zsh" | "fish" | "ksh" => Some("text/x-shellscript; charset=utf-8"),
        "sql" => Some("application/sql; charset=utf-8"),
        "proto" => Some("text/plain; charset=utf-8"),
        "graphql" | "gql" => Some("text/plain; charset=utf-8"),
        "diff" | "patch" => Some("text/x-diff; charset=utf-8"),
        "vue" | "svelte" => Some("text/plain; charset=utf-8"),

        // Web & Markup
        "html" | "htm" | "xhtml" => Some("text/html; charset=utf-8"),
        "css" | "scss" | "sass" | "less" => Some("text/css; charset=utf-8"),
        "xml" => Some("application/xml; charset=utf-8"),
        "svg" => Some("image/svg+xml"),

        // Docs & Data
        "txt" | "text" | "log" => Some("text/plain; charset=utf-8"),
        "md" | "markdown" | "mdown" | "mkd" => Some("text/markdown; charset=utf-8"),
        "csv" => Some("text/csv; charset=utf-8"),
        "tsv" => Some("text/tab-separated-values; charset=utf-8"),
        "pdf" => Some("application/pdf"),
        "lock" | "sum" | "mod" => Some("text/plain; charset=utf-8"),

        _ => None,
    }
}

pub fn format_with_charset(mime: &str) -> String {
    if (mime.starts_with("text/")
        || mime.contains("json")
        || mime.contains("xml")
        || mime.contains("javascript")
        || mime.contains("yaml")
        || mime.contains("toml"))
        && !mime.contains("charset")
    {
        format!("{mime}; charset=utf-8")
    } else {
        mime.to_string()
    }
}

/// Detect Content-Type from a path and optional content sniff bytes.
pub fn detect_content_type(path: &Path, sniff_bytes: Option<&[u8]>) -> String {
    let fast = detect_content_type_from_path(path);
    if fast != "application/octet-stream" {
        return fast;
    }

    if let Some(bytes) = sniff_bytes {
        if !bytes.is_empty() {
            // If it contains null bytes, treat as binary octet-stream
            if bytes.iter().take(1024).any(|&b| b == 0) {
                return "application/octet-stream".to_string();
            }
            // If it's valid UTF-8, treat as text
            if std::str::from_utf8(bytes).is_ok() {
                return "text/plain; charset=utf-8".to_string();
            }
        }
    }

    "application/octet-stream".to_string()
}

/// Detect Content-Type purely from file path / name / extension.
pub fn detect_content_type_from_path(path: &Path) -> String {
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let effective_name = strip_backup_suffix(filename);
    let lower_name = effective_name.to_ascii_lowercase();

    // 1. Check known exact filenames
    if let Some(ct) = exact_filename_content_type(&lower_name) {
        return ct.to_string();
    }

    // 2. Prefix matches for filenames like `caddyfile.*`, `dockerfile.*`
    if lower_name.starts_with("caddyfile.") || lower_name == "caddyfile" {
        return "text/plain; charset=utf-8".to_string();
    }
    if lower_name.starts_with("dockerfile.") || lower_name == "dockerfile" {
        return "text/plain; charset=utf-8".to_string();
    }
    if lower_name.starts_with(".env") {
        return "text/plain; charset=utf-8".to_string();
    }

    // 3. Extension matches
    let ext = effective_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    if let Some(ct) = extension_content_type(&ext) {
        return ct.to_string();
    }

    // 4. Fallback to mime_guess on effective name
    if let Some(m) = mime_guess::from_path(effective_name).first() {
        let m_str = m.to_string();
        if m_str != "application/octet-stream" {
            return format_with_charset(&m_str);
        }
    }

    "application/octet-stream".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_caddyfiles_and_timestamped_backups() {
        assert_eq!(
            detect_content_type_from_path(Path::new("Caddyfile")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("backups/caddy/Caddyfile.20261102-214513")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("Caddyfile.20261102_215408")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("caddyfile.bak")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("my-site.caddyfile")),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn detects_extensionless_and_dotfiles() {
        assert_eq!(
            detect_content_type_from_path(Path::new("Dockerfile")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("Dockerfile.dev")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("Makefile")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new(".env")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new(".env.production")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new(".gitignore")),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn detects_backed_up_code_and_config() {
        assert_eq!(
            detect_content_type_from_path(Path::new("server.py.old")),
            "text/x-python; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("nginx.conf.bak")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type_from_path(Path::new("config.yaml.20261102")),
            "text/yaml; charset=utf-8"
        );
    }

    #[test]
    fn detects_sniffed_text_vs_binary() {
        let text_bytes = b"server { listen 80; }";
        assert_eq!(
            detect_content_type(Path::new("unknown_file"), Some(text_bytes)),
            "text/plain; charset=utf-8"
        );

        let binary_bytes = b"\x7fELF\x02\x01\x01\x00\x00\x00";
        assert_eq!(
            detect_content_type(Path::new("unknown_file"), Some(binary_bytes)),
            "application/octet-stream"
        );
    }
}
