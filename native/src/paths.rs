use anyhow::{Result, bail, ensure};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub fn normalized(path: &str) -> Result<String> {
    ensure!(
        !path.is_empty() && path.len() <= 4096,
        "Invalid path length"
    );
    ensure!(
        !path.contains(['\\', ':', '\0']) && !path.chars().any(char::is_control),
        "Invalid path characters"
    );
    let parsed = Path::new(path);
    ensure!(!parsed.is_absolute(), "Absolute path refused");
    let parts: Vec<_> = parsed
        .components()
        .map(|p| match p {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    ensure!(
        parts.iter().all(|p| p.is_some()),
        "Non-relative path refused"
    );
    let normalized = parts.into_iter().flatten().collect::<Vec<_>>().join("/");
    ensure!(normalized == path, "Noncanonical path refused");
    for part in path.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let numbered_device = stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9');
        ensure!(
            !matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) && !numbered_device
                && !part.ends_with(['.', ' '])
                && !part.contains(['<', '>', '"', '|', '?', '*']),
            "Nonportable or reserved path refused"
        );
    }
    ensure!(!protected(path), "Protected path refused");
    Ok(normalized)
}

pub fn protected(path: &str) -> bool {
    path.split('/').any(|part| {
        let p = part.to_ascii_lowercase();
        matches!(
            p.as_str(),
            ".git"
                | ".yoke"
                | ".code-intelligence"
                | "node_modules"
                | "target"
                | ".ssh"
                | ".aws"
                | ".azure"
                | "__pycache__"
                | ".venv"
                | ".npmrc"
                | ".pypirc"
                | ".netrc"
                | "_netrc"
        ) || p.starts_with(".env")
            || p.starts_with("credentials.")
            || matches!(
                p.as_str(),
                "credentials" | "credentials.json" | "id_rsa" | "id_ed25519"
            )
            || p.ends_with(".pem")
            || p.ends_with(".key")
            || p.ends_with(".p12")
            || p.ends_with(".pfx")
            || p.ends_with(".keystore")
    })
}
pub fn obvious_secret(text: &str) -> bool {
    use std::sync::OnceLock;
    static TOKENS: OnceLock<regex::Regex> = OnceLock::new();
    text.contains("-----BEGIN PRIVATE KEY-----") || text.contains("-----BEGIN RSA PRIVATE KEY-----") || text.contains("-----BEGIN EC PRIVATE KEY-----") || text.contains("-----BEGIN OPENSSH PRIVATE KEY-----") || TOKENS.get_or_init(|| regex::Regex::new(r"(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[A-Z0-9]{16}|sk-[A-Za-z0-9]{32,})").expect("static secret pattern")).is_match(text)
}

pub fn scoped(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = normalized(relative)?;
    let mut current = root.to_path_buf();
    for part in relative.split('/') {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) => ensure!(!is_link(&meta), "Symbolic link or reparse point refused"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    ensure!(current.starts_with(root), "Path escaped workspace");
    Ok(current)
}

pub fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

pub fn utf8_range(text: &str, start: usize, end: usize) -> Result<&str> {
    if start > end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        bail!("Invalid UTF-8 byte range");
    }
    Ok(&text[start..end])
}

pub fn line_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = vec![0];
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\r' => {
                cursor += if bytes.get(cursor + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                starts.push(cursor);
            }
            b'\n' => {
                cursor += 1;
                starts.push(cursor);
            }
            _ => cursor += 1,
        }
    }
    starts
}
