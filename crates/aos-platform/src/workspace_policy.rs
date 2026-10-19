//! Shared, bounded policy for automatic project discovery and model context.
//! This is a conservative content filter, not a complete DLP or filesystem sandbox.
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_SCAN_FILES: usize = 10_000;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SecretFinding {
    pub line: u32,
    pub rule: &'static str,
}
/// Never return snippets, values, captured tokens or hashes of individual secrets.
pub fn secret_findings(text: &str) -> Vec<SecretFinding> {
    static MARKERS: std::sync::OnceLock<aho_corasick::AhoCorasick> = std::sync::OnceLock::new();
    let markers = MARKERS.get_or_init(|| {
        aho_corasick::AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .build([
                "private key",
                "ghp_",
                "github_pat_",
                "sk-",
                "AKIA",
                "ASIA",
                "password",
                "passwd",
                "token",
                "api_key",
                "apikey",
                "secret",
                "authorization",
            ])
            .expect("static credential marker patterns")
    });
    if !markers.is_match(text) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if !markers.is_match(line) {
            continue;
        }
        let rule = if let Some((_, tail)) = line.split_once("-----BEGIN ") {
            tail.split_once("-----").and_then(|(kind, _)| {
                (kind.ends_with("PRIVATE KEY")
                    && kind.chars().all(|c| c.is_ascii_uppercase() || c == ' '))
                .then_some("private_key")
            })
        } else {
            None
        };
        let rule = rule.or_else(|| {
            if !["ghp_", "github_pat_", "sk-", "AKIA", "ASIA"]
                .iter()
                .any(|prefix| line.contains(prefix))
            {
                return None;
            }
            line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .find_map(|word| {
                    if ["ghp_", "github_pat_", "sk-"]
                        .iter()
                        .any(|prefix| word.strip_prefix(prefix).is_some_and(|v| v.len() >= 20))
                    {
                        Some("credential_token")
                    } else if word.len() == 20
                        && (word.starts_with("AKIA") || word.starts_with("ASIA"))
                        && word
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                    {
                        Some("access_key_id")
                    } else {
                        None
                    }
                })
        });
        let rule = rule.or_else(|| {
            let (left, right) = line.split_once('=').or_else(|| line.split_once(':'))?;
            let key = left
                .trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .next()?
                .to_ascii_lowercase();
            if ![
                "password",
                "passwd",
                "token",
                "api_key",
                "apikey",
                "secret",
                "access_token",
                "client_secret",
                "authorization",
            ]
            .contains(&key.as_str())
            {
                return None;
            }
            let right = right.trim();
            let first = right.chars().next()?;
            let literal = if first == '\'' || first == '"' {
                right[1..].split(first).next()?
            } else {
                right.trim_end_matches([',', ';'])
            };
            let lower = literal.to_ascii_lowercase();
            let placeholder = [
                "changeme",
                "example",
                "placeholder",
                "redacted",
                "password",
                "test",
                "dummy",
                "none",
                "null",
            ]
            .contains(&lower.as_str())
                || literal.starts_with("${")
                || (literal.starts_with('<') && literal.ends_with('>'));
            (literal.len() >= 12
                && !placeholder
                && !literal.chars().any(char::is_whitespace)
                && (first == '\''
                    || first == '"'
                    || literal
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_-+/=".contains(c))))
            .then_some("credential_assignment")
        });
        if let Some(rule) = rule {
            findings.push(SecretFinding {
                line: index as u32 + 1,
                rule,
            });
        }
        if findings.len() == 8 {
            break;
        }
    }
    findings
}
pub fn forbidden(path: &Path) -> bool {
    path.components().any(|component| {
        let raw_name = component.as_os_str().to_string_lossy();
        if !secret_findings(&raw_name).is_empty() {
            return true;
        }
        let name = raw_name.to_ascii_lowercase();
        matches!(
            name.as_str(),
            ".git"
                | "node_modules"
                | "target"
                | "dist"
                | ".venv"
                | "venv"
                | "__pycache__"
                | ".cache"
                | ".env"
                | "credentials.json"
                | "id_rsa"
                | "id_ed25519"
        ) || name.starts_with(".env.")
            || [".pem", ".key", ".p12", ".pfx"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
    })
}
fn link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
pub struct WorkspacePolicy {
    root: PathBuf,
    ignores: HashMap<PathBuf, Gitignore>,
}
impl WorkspacePolicy {
    pub fn new(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("workspace root is not a directory".into());
        }
        Ok(Self {
            root,
            ignores: HashMap::new(),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn rules(&mut self, parent: &Path) -> Result<&Gitignore, String> {
        if !self.ignores.contains_key(parent) {
            if self.ignores.len() >= 128 {
                self.ignores.clear();
            }
            let mut total_rules = 0;
            let mut builder = GitignoreBuilder::new(&self.root);
            let mut dirs = parent
                .ancestors()
                .take_while(|p| p.starts_with(&self.root))
                .collect::<Vec<_>>();
            dirs.reverse();
            for dir in dirs {
                for name in [".gitignore", ".ignore"] {
                    let path = dir.join(name);
                    match path.symlink_metadata() {
                        Ok(metadata) => {
                            if link(&metadata) || !metadata.is_file() || metadata.len() > 64 * 1024
                            {
                                return Err(
                                    "ignore rules are linked, oversized or not a file".into()
                                );
                            }
                            total_rules += metadata.len();
                            if total_rules > 128 * 1024 {
                                return Err("ignore rule chain exceeds 128 KiB".into());
                            }
                            if let Some(error) = builder.add(path) {
                                return Err(format!("invalid project ignore rules: {error}"));
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e.to_string()),
                    }
                }
            }
            self.ignores.insert(
                parent.to_path_buf(),
                builder.build().map_err(|e| e.to_string())?,
            );
        }
        Ok(&self.ignores[parent])
    }
    pub fn checked(&mut self, raw: &str, directory: bool) -> Result<PathBuf, String> {
        let relative = Path::new(raw);
        if raw.is_empty()
            || raw.contains(['\\', ':'])
            || raw.chars().any(char::is_control)
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || forbidden(relative)
        {
            return Err("invalid or excluded project path".into());
        }
        let mut path = self.root.clone();
        for part in relative.components() {
            path.push(part);
            if link(&path.symlink_metadata().map_err(|e| e.to_string())?) {
                return Err("linked project paths are excluded".into());
            }
        }
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(&self.root)
            || canonical == self.root
            || directory != canonical.is_dir()
            || (!directory && !canonical.is_file())
        {
            return Err("project path escapes workspace or has wrong type".into());
        }
        let parent = canonical
            .parent()
            .ok_or("project path has no parent")?
            .to_path_buf();
        if self
            .rules(&parent)?
            .matched_path_or_any_parents(&canonical, directory)
            .is_ignore()
        {
            return Err("project path is ignored".into());
        }
        Ok(canonical)
    }
    /// Discovery already obtained the entry metadata while walking checked
    /// directories. Canonical identity must still equal the enumerated path:
    /// this rejects redirected ancestors (inside or outside this workspace)
    /// without restatting every ancestor for every file. Like checked(), this
    /// is an observation, not atomic isolation against concurrent path swaps.
    fn checked_enumerated(
        &mut self,
        relative: &str,
        path: &Path,
        metadata: &std::fs::Metadata,
    ) -> Result<(), String> {
        if relative.is_empty()
            || relative.contains(['\\', ':'])
            || relative.chars().any(char::is_control)
            || Path::new(relative)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || forbidden(Path::new(relative))
            || link(metadata)
            || (!metadata.is_dir() && !metadata.is_file())
            || self.root.join(relative) != path
        {
            return Err("invalid, linked or excluded project entry".into());
        }
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        if canonical != path || !canonical.starts_with(&self.root) {
            return Err("project entry changed identity or crosses a linked path".into());
        }
        let parent = canonical.parent().ok_or("project entry has no parent")?;
        if self
            .rules(parent)?
            .matched_path_or_any_parents(&canonical, metadata.is_dir())
            .is_ignore()
        {
            return Err("project path is ignored".into());
        }
        Ok(())
    }
    pub fn read_text(&mut self, relative: &str) -> Result<String, String> {
        let path = self.checked(relative, false)?;
        let text = read_bounded(&path)?;
        if !secret_findings(&text).is_empty() {
            return Err("project content blocked by credential/private key policy".into());
        }
        Ok(text)
    }
}
fn read_bytes(path: &Path, allowance: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take((MAX_FILE_BYTES + 1).min(allowance + 1))
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
fn decode_text(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.contains(&0) {
        return Err("project file is binary".into());
    }
    String::from_utf8(bytes).map_err(|_| "project file is not UTF-8 text".into())
}
fn read_bounded(path: &Path) -> Result<String, String> {
    let bytes = read_bytes(path, MAX_FILE_BYTES)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("project file exceeds 1 MiB".into());
    }
    decode_text(bytes)
}

#[derive(Clone, Copy)]
pub struct ScanLimits {
    pub files: usize,
    pub bytes: u64,
}
impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            files: MAX_SCAN_FILES,
            bytes: MAX_SCAN_BYTES,
        }
    }
}
#[derive(Debug, Default, Serialize)]
pub struct ScanReport {
    pub complete: bool,
    pub files_read: usize,
    pub eligible_files: usize,
    pub bytes_read: u64,
    pub visited_entries: usize,
    pub skipped: BTreeMap<&'static str, usize>,
    pub findings: Vec<FileFinding>,
    pub finding_files: usize,
    pub findings_truncated: bool,
}
#[derive(Debug, Serialize)]
pub struct FileFinding {
    pub path: String,
    pub findings: Vec<SecretFinding>,
}
impl ScanReport {
    fn skip(&mut self, reason: &'static str) {
        *self.skipped.entry(reason).or_default() += 1;
    }
}
/// One native engine for inventory and search: no subprocess or shell arguments.
/// Returning false stops at a result limit; incomplete scans are always explicit.
pub fn scan(
    policy: &mut WorkspacePolicy,
    start: &Path,
    limits: ScanLimits,
    mut visit: impl FnMut(&str, &str) -> bool,
) -> Result<ScanReport, String> {
    let start = start.canonicalize().map_err(|e| e.to_string())?;
    if !start.starts_with(policy.root()) {
        return Err("scan root escapes workspace".into());
    }
    let limits = ScanLimits {
        files: limits.files.clamp(1, MAX_SCAN_FILES),
        bytes: limits.bytes.clamp(1, MAX_SCAN_BYTES),
    };
    let mut report = ScanReport {
        complete: true,
        ..Default::default()
    };
    let begun = Instant::now();
    let mut stack = vec![start];
    while let Some(path) = stack.pop() {
        if begun.elapsed() > Duration::from_secs(10)
            || report.visited_entries >= 50_000
            || report.files_read >= limits.files
        {
            report.complete = false;
            report.skip("scan_limit");
            break;
        }
        report.visited_entries += 1;
        let metadata = match path.symlink_metadata() {
            Ok(m) => m,
            Err(_) => {
                report.complete = false;
                report.skip("unavailable");
                continue;
            }
        };
        if link(&metadata) {
            report.skip("linked_path");
            continue;
        }
        if !metadata.is_dir() && !metadata.is_file() {
            report.skip("non_file");
            continue;
        }
        let relative = path
            .strip_prefix(policy.root())
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.is_empty() {
            if relative.split('/').count() > 64 {
                report.complete = false;
                report.skip("depth_limit");
                continue;
            }
            if forbidden(Path::new(&relative)) {
                report.skip("excluded_path");
                continue;
            }
            if let Err(error) = policy.checked_enumerated(&relative, &path, &metadata) {
                if error == "project path is ignored" {
                    report.skip("ignored");
                } else {
                    return Err(error);
                }
                continue;
            }
        }
        if metadata.is_dir() {
            let entries = std::fs::read_dir(&path)
                .map_err(|e| e.to_string())?
                .take(20_001)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            if entries.len() > 20_000 {
                report.complete = false;
                report.skip("directory_limit");
                continue;
            }
            let mut entries = entries
                .into_iter()
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            entries.sort();
            stack.extend(entries.into_iter().rev());
            continue;
        }
        if !metadata.is_file() {
            report.skip("non_file");
            continue;
        }
        if metadata.len() > MAX_FILE_BYTES {
            report.skip("file_size");
            continue;
        }
        if report.bytes_read + metadata.len() > limits.bytes {
            report.complete = false;
            report.skip("byte_limit");
            break;
        }
        let bytes = match read_bytes(&path, limits.bytes - report.bytes_read) {
            Ok(bytes) => bytes,
            Err(_) => {
                report.complete = false;
                report.skip("unreadable");
                continue;
            }
        };
        report.files_read += 1;
        report.bytes_read += bytes.len() as u64;
        if report.bytes_read > limits.bytes {
            report.complete = false;
            report.skip("byte_limit");
            break;
        }
        if bytes.len() as u64 > MAX_FILE_BYTES {
            report.skip("file_size");
            continue;
        }
        let text = match decode_text(bytes) {
            Ok(text) => text,
            Err(_) => {
                report.skip("binary");
                continue;
            }
        };
        let findings = secret_findings(&text);
        if !findings.is_empty() {
            report.finding_files += 1;
            if report.findings.len() < 256 {
                report.findings.push(FileFinding {
                    path: relative,
                    findings,
                });
            } else {
                report.findings_truncated = true;
            }
            continue;
        }
        report.eligible_files += 1;
        if !visit(&relative, &text) {
            report.complete = false;
            report.skip("result_limit");
            break;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "aos-project-policy-{:032x}",
                rand::random::<u128>()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn write(&self, path: &str, text: &str) {
            let file = self.0.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn credentials_are_detected_without_returning_values() {
        let token = format!("ghp_{}", "Z".repeat(36));
        let text=format!("const answer=42;\napi_key = '{token}'\npassword = 'long-hardcoded-value'\n-----BEGIN RSA PRIVATE KEY-----\n");
        let findings = secret_findings(&text);
        assert_eq!(findings.len(), 3);
        let report = serde_json::to_string(&findings).unwrap();
        assert!(!report.contains(&token));
        assert!(!report.contains("long-hardcoded"));
        assert_eq!(findings[0].line, 2);
        assert!(secret_findings(
            "api_key = '${API_KEY}'\npassword = '<set-me>'\nlet token = response.get_token();\n"
        )
        .is_empty());
    }
    #[test]
    fn nested_ignores_and_credential_names_apply_without_git_repository() {
        let f = Fixture::new();
        f.write(".gitignore", "ignored/\n");
        f.write("src/.ignore", "*.secret\n");
        for path in [
            "src/ok.rs",
            "ignored/no.rs",
            "src/token.secret",
            ".env",
            "src/key.pem",
            "node_modules/lib.js",
        ] {
            f.write(path, "marker\n");
        }
        f.write(&format!("src/ghp_{}.rs", "Z".repeat(36)), "marker\n");
        let mut policy = WorkspacePolicy::new(&f.0).unwrap();
        let mut seen = Vec::new();
        let root = policy.root().to_path_buf();
        let report = scan(&mut policy, &root, ScanLimits::default(), |path, _| {
            seen.push(path.to_string());
            true
        })
        .unwrap();
        assert!(seen.contains(&"src/ok.rs".into()));
        for path in [
            "ignored/no.rs",
            "src/token.secret",
            ".env",
            "src/key.pem",
            "node_modules/lib.js",
        ] {
            assert!(!seen.contains(&path.into()), "{path}");
        }
        assert!(!seen.iter().any(|p| p.contains(&"Z".repeat(36))));
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains(&"Z".repeat(36)));
        assert!(report.complete);
        assert!(policy.checked("src/../.env", false).is_err());
    }
    #[test]
    fn whole_file_is_withheld_when_a_secret_is_on_a_different_line() {
        let f = Fixture::new();
        f.write(
            "source.rs",
            &format!("marker\nconst token = \"ghp_{}\";\n", "X".repeat(36)),
        );
        let mut policy = WorkspacePolicy::new(&f.0).unwrap();
        assert!(policy.read_text("source.rs").is_err());
        let root = policy.root().to_path_buf();
        let report = scan(&mut policy, &root, ScanLimits::default(), |_, _| {
            panic!("secret file reached context")
        })
        .unwrap();
        assert_eq!(report.finding_files, 1);
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains(&"X".repeat(36)));
    }
    #[test]
    fn budgets_and_oversized_ignore_rules_fail_visibly() {
        let f = Fixture::new();
        f.write("one.rs", "one\n");
        f.write("two.rs", "two\n");
        let mut policy = WorkspacePolicy::new(&f.0).unwrap();
        let root = policy.root().to_path_buf();
        let report = scan(
            &mut policy,
            &root,
            ScanLimits {
                files: 1,
                bytes: 100,
            },
            |_, _| true,
        )
        .unwrap();
        assert!(!report.complete);
        assert_eq!(report.files_read, 1);
        f.write(".ignore", &"x".repeat(65_537));
        let mut policy = WorkspacePolicy::new(&f.0).unwrap();
        assert!(scan(&mut policy, &root, ScanLimits::default(), |_, _| true).is_err());
    }
    #[test]
    fn links_are_not_followed_or_accepted_as_explicit_context() {
        let f = Fixture::new();
        f.write("actual.rs", "safe");
        let outside = Fixture::new();
        outside.write("secret.rs", "marker");
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(&outside.0, f.0.join("linked"));
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_dir(&outside.0, f.0.join("linked"));
        if result.is_err() {
            return;
        } // Windows may require Developer Mode for symlinks.
        let mut policy = WorkspacePolicy::new(&f.0).unwrap();
        assert!(policy.checked("linked/secret.rs", false).is_err());
        let root = policy.root().to_path_buf();
        let mut seen = Vec::new();
        scan(&mut policy, &root, ScanLimits::default(), |p, _| {
            seen.push(p.to_string());
            true
        })
        .unwrap();
        assert!(!seen.iter().any(|p| p.contains("linked")));
    }
    #[cfg(unix)]
    #[test]
    fn special_files_cannot_block_an_explicit_context_read() {
        let f = Fixture::new();
        let _socket = std::os::unix::net::UnixListener::bind(f.0.join("socket")).unwrap();
        let mut policy = WorkspacePolicy::new(&f.0).unwrap();
        assert!(policy.checked("socket", false).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn windows_junction_is_excluded_without_following_its_target() {
        struct Junction(PathBuf);
        impl Drop for Junction {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir(&self.0);
            }
        }
        let f = Fixture::new();
        let outside = Fixture::new();
        f.write("safe.rs", "safe\n");
        outside.write("outside.rs", "outside-marker\n");
        let path = f.0.join("linked");
        let powershell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        // Values travel through environment variables, not command interpolation.
        let result = std::process::Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:AOS_TEST_JUNCTION -Target $env:AOS_TEST_TARGET | Out-Null"])
            .env("AOS_TEST_JUNCTION", &path)
            .env("AOS_TEST_TARGET", &outside.0)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "could not create a generated local NTFS junction fixture: stdout={}, stderr={}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let _junction = Junction(path);
        let mut policy = WorkspacePolicy::new(&f.0).unwrap();
        assert!(policy.read_text("linked/outside.rs").is_err());
        let redirected = policy.root().join("linked/outside.rs");
        let metadata = redirected.symlink_metadata().unwrap();
        assert!(policy
            .checked_enumerated("linked/outside.rs", &redirected, &metadata)
            .is_err());
        let root = policy.root().to_path_buf();
        let mut paths = Vec::new();
        let report = scan(&mut policy, &root, ScanLimits::default(), |path, _| {
            paths.push(path.to_string());
            true
        })
        .unwrap();
        assert!(report.complete);
        assert_eq!(report.skipped.get("linked_path"), Some(&1));
        assert_eq!(paths, vec!["safe.rs"]);
        assert_eq!(
            std::fs::read_to_string(outside.0.join("outside.rs")).unwrap(),
            "outside-marker\n"
        );
    }
}
