//! Explicit host-selected private raw logs. No capture, polling, or writes.
use super::{documents::ProjectRoot, io};
use crate::{
    capture::Deadline,
    model::{hash, identity},
};
use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 64_000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogSpec {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub logs: Vec<LogSpec>,
    #[serde(default = "default_maximum")]
    pub max_window_bytes: usize,
}
fn default_maximum() -> usize {
    MAX_BYTES
}
struct BoundLog {
    spec: LogSpec,
    parent: PathBuf,
    parent_identity: String,
}
pub struct Access {
    logs: BTreeMap<String, BoundLog>,
    roots: Vec<(PathBuf, String)>,
    maximum: usize,
    binding: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub observation_id: String,
    pub log_id: String,
    pub label: String,
    pub opened_file_identity: String,
    // Decimal strings preserve exact offsets beyond JavaScript's integer range.
    pub initial_size_bytes: String,
    pub final_size_bytes: String,
    pub start_byte: String,
    pub end_byte: String,
    pub byte_length: usize,
    pub short_read: bool,
    pub immutable_snapshot: bool,
    pub current_path_generation_guaranteed: bool,
    pub sha256: String,
    pub base64: String,
    pub text: String,
    pub text_lossy: bool,
}
fn key(path: &Path) -> Result<String> {
    let text = path.to_str().context("Host log path must be Unicode")?;
    Ok(if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text.to_owned()
    })
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.logs.is_empty() && self.logs.len() <= 16,
            "dashboard_log_selection_capacity"
        );
        ensure!(
            (1..=MAX_BYTES).contains(&self.max_window_bytes),
            "dashboard_log_window_capacity"
        );
        let mut ids = BTreeSet::new();
        for log in &self.logs {
            crate::project_session::label(&log.id)?;
            ensure!(
                ids.insert(&log.id) && !log.label.is_empty() && log.label.len() <= 200,
                "dashboard_log_identity_or_label_invalid"
            );
            ensure!(
                log.path.is_absolute() && log.path.file_name().is_some(),
                "dashboard_log_absolute_file_required"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_BYTES,
            "dashboard_log_selection_byte_capacity"
        );
        Ok(())
    }
}
impl Access {
    /// Only directory identities are captured. A selected log may not exist yet;
    /// setup neither reads payloads nor creates a file or starts a logger.
    pub fn open(config: &Config, roots: &[ProjectRoot], d: &Deadline) -> Result<Self> {
        d.check()?;
        config.validate()?;
        ensure!(cfg!(windows), "dashboard_guarded_host_logs_windows_only");
        ensure!(
            !roots.is_empty() && roots.len() <= 101,
            "dashboard_log_project_capacity"
        );
        let guards = roots
            .iter()
            .map(|r| io::guarded_log_directory(&r.path, d))
            .collect::<Result<Vec<_>>>()?;
        let roots = guards
            .iter()
            .map(|g| (g.path.clone(), g.identity.clone()))
            .collect::<Vec<_>>();
        let mut logs = BTreeMap::new();
        let mut targets = BTreeSet::new();
        for log in &config.logs {
            d.check()?;
            let parent = io::guarded_log_directory(
                log.path.parent().context("Host log parent required")?,
                d,
            )?;
            let path = parent
                .path
                .join(log.path.file_name().context("Host log leaf required")?);
            let canonical_key = key(&path)?;
            ensure!(
                targets.insert(canonical_key.clone()),
                "dashboard_log_duplicate_target"
            );
            for (_, root_identity) in &roots {
                ensure!(
                    !io::has_ancestor_identity(&parent, root_identity, d)?,
                    "dashboard_log_inside_analyzed_root"
                );
            }
            logs.insert(
                log.id.clone(),
                BoundLog {
                    spec: LogSpec {
                        path,
                        ..log.clone()
                    },
                    parent: parent.path.clone(),
                    parent_identity: parent.identity.clone(),
                },
            );
        }
        let binding = identity(&(
            "dashboard_host_logs_v1",
            config,
            &roots,
            logs.iter()
                .map(|(id, log)| (id, &log.parent, &log.parent_identity))
                .collect::<Vec<_>>(),
        ));
        Ok(Self {
            logs,
            roots,
            maximum: config.max_window_bytes,
            binding,
        })
    }
    pub fn binding(&self) -> &str {
        &self.binding
    }
    pub fn selections(&self) -> impl Iterator<Item = &LogSpec> {
        self.logs.values().map(|l| &l.spec)
    }
    pub fn read(&self, id: &str, maximum: Option<usize>, d: &Deadline) -> Result<Observation> {
        d.check()?;
        let selected = self.logs.get(id).context("Host log is not selected")?;
        let maximum = maximum.unwrap_or(self.maximum);
        ensure!(
            (1..=self.maximum).contains(&maximum),
            "dashboard_log_window_not_granted"
        );
        let mut guards = Vec::new();
        for (path, id) in &self.roots {
            let guard = io::guarded_log_directory(path, d)?;
            ensure!(&guard.identity == id, "Host log analyzed root changed");
            guards.push(guard);
        }
        let parent = io::guarded_log_directory(&selected.parent, d)?;
        ensure!(
            parent.identity == selected.parent_identity,
            "Host log parent changed"
        );
        guards.push(parent);
        let window = io::read_log_tail(&selected.spec.path, maximum, d)?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce)?;
        let text_lossy = std::str::from_utf8(&window.bytes).is_err();
        let end = window
            .start
            .checked_add(window.bytes.len() as u64)
            .context("Host log range overflow")?;
        Ok(Observation {
            observation_id: hash(nonce),
            log_id: id.into(),
            label: selected.spec.label.clone(),
            opened_file_identity: window.identity,
            initial_size_bytes: window.initial_size.to_string(),
            final_size_bytes: window.final_size.to_string(),
            start_byte: window.start.to_string(),
            end_byte: end.to_string(),
            byte_length: window.bytes.len(),
            short_read: (window.bytes.len() as u64) < window.initial_size - window.start,
            immutable_snapshot: false,
            current_path_generation_guaranteed: false,
            sha256: hash(&window.bytes),
            base64: base64::engine::general_purpose::STANDARD.encode(&window.bytes),
            text: String::from_utf8_lossy(&window.bytes).into_owned(),
            text_lossy,
        })
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Config, Vec<ProjectRoot>) {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        let private = fixture.path().join("private");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&private).unwrap();
        let config = Config {
            logs: vec![LogSpec {
                id: "session-log".into(),
                label: "Private session log".into(),
                path: private.join("session.log"),
            }],
            max_window_bytes: 8,
        };
        (
            fixture,
            config,
            vec![ProjectRoot {
                name: "default".into(),
                path: source,
            }],
        )
    }
    #[test]
    fn no_automatic_capture_then_exact_split_unicode_and_distinct_observations() {
        let (_fixture, config, roots) = setup();
        let d = Deadline::new(10_000);
        let access = Access::open(&config, &roots, &d).unwrap();
        let path = &config.logs[0].path;
        assert!(!path.exists());
        assert!(access.read("session-log", None, &d).is_err());
        std::fs::write(path, b"prefix\xf0\x9f\xa6\x80\r\n").unwrap();
        let a = access.read("session-log", Some(4), &d).unwrap();
        let b = access.read("session-log", Some(4), &d).unwrap();
        assert_eq!(a.start_byte, "8");
        assert_eq!(a.end_byte, "12");
        assert_eq!(a.sha256, hash(b"\xa6\x80\r\n"));
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(a.base64)
                .unwrap(),
            b"\xa6\x80\r\n"
        );
        assert!(
            a.text_lossy
                && !a.immutable_snapshot
                && !a.current_path_generation_guaranteed
                && !a.short_read
        );
        assert_ne!(a.observation_id, b.observation_id);
        assert_eq!(a.sha256, b.sha256);
        for n in [0, 9, usize::MAX] {
            assert!(access.read("session-log", Some(n), &d).is_err());
        }
        assert!(access.read("not-selected", None, &d).is_err());
        assert!(access.read("session-log", None, &Deadline::new(0)).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"prefix\xf0\x9f\xa6\x80\r\n");
    }
    #[test]
    fn selection_rejects_analyzed_roots_duplicates_and_invalid_budgets() {
        let (_fixture, mut config, roots) = setup();
        let d = Deadline::new(10_000);
        for n in [0, 64001, usize::MAX] {
            config.max_window_bytes = n;
            assert!(config.validate().is_err());
        }
        config.max_window_bytes = 8;
        let mut duplicate = config.logs[0].clone();
        duplicate.id = "different-id".into();
        config.logs.push(duplicate);
        assert!(Access::open(&config, &roots, &d).is_err());
        config.logs.pop();
        config.logs[0].path = roots[0].path.join("analyzed.log");
        assert!(Access::open(&config, &roots, &d).is_err());
        assert!(!config.logs[0].path.exists());
    }
    #[test]
    fn replaced_parent_or_analyzed_root_invalidates_selected_log() {
        for replace_root in [false, true] {
            let (_fixture, config, roots) = setup();
            let d = Deadline::new(10_000);
            std::fs::write(&config.logs[0].path, b"original").unwrap();
            let access = Access::open(&config, &roots, &d).unwrap();
            let path = if replace_root {
                roots[0].path.clone()
            } else {
                config.logs[0].path.parent().unwrap().to_path_buf()
            };
            std::fs::rename(&path, path.with_extension("original")).unwrap();
            std::fs::create_dir(&path).unwrap();
            assert!(access.read("session-log", None, &d).is_err());
        }
    }

    #[test]
    fn analyzed_root_admission_uses_physical_ancestry_including_unicode_aliases() {
        let (fixture, mut config, _) = setup();
        let root = fixture.path().join("Änalyzed");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("nested")).unwrap();
        let roots = vec![ProjectRoot {
            name: "default".into(),
            path: root,
        }];
        config.logs[0].path = fixture.path().join("änalyzed/nested/private.log");
        assert!(
            config.logs[0].path.parent().unwrap().is_dir(),
            "Fixture requires the existing case-insensitive directory alias"
        );
        assert!(Access::open(&config, &roots, &Deadline::new(10_000)).is_err());
        let sibling = fixture.path().join("Änalyzed-sibling");
        std::fs::create_dir(&sibling).unwrap();
        config.logs[0].path = sibling.join("private.log");
        assert!(Access::open(&config, &roots, &Deadline::new(10_000)).is_ok());
    }
}
