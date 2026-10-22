//! Explicit negative source selections. Rules never grant reads, writes or execution.
use crate::{capture::Deadline, project_session::HostProfile};
use anyhow::{Result, ensure};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const MAX_GROUPS: usize = 16;
pub const MAX_PATTERNS: usize = 128;
pub const MAX_PATTERN_BYTES: usize = 512;
pub const MAX_TOTAL_BYTES: usize = 32768;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Group {
    pub patterns: Vec<String>,
    /// Concrete matching semantics, pinned at capture rather than inferred at replay.
    pub case_insensitive: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Policy {
    GitignoreV1 { groups: Vec<Group> },
}
impl Policy {
    pub fn groups(&self) -> &[Group] {
        match self {
            Self::GitignoreV1 { groups } => groups,
        }
    }
    pub(crate) fn canonical(&self) -> Result<Option<Self>> {
        let groups = self.groups();
        ensure!(groups.len() <= MAX_GROUPS, "source_exclusions_group_budget");
        let mut count = 0usize;
        let mut bytes = 0usize;
        for group in groups {
            count = count
                .checked_add(group.patterns.len())
                .ok_or_else(|| anyhow::anyhow!("source_exclusions_pattern_budget"))?;
            ensure!(count <= MAX_PATTERNS, "source_exclusions_pattern_budget");
            for pattern in &group.patterns {
                ensure!(
                    !pattern.is_empty()
                        && pattern.len() <= MAX_PATTERN_BYTES
                        && !pattern.contains(['\0', '\r', '\n']),
                    "source_exclusions_pattern_invalid"
                );
                bytes = bytes
                    .checked_add(pattern.len())
                    .ok_or_else(|| anyhow::anyhow!("source_exclusions_byte_budget"))?;
                ensure!(bytes <= MAX_TOTAL_BYTES, "source_exclusions_byte_budget");
            }
        }
        let groups: Vec<_> = groups
            .iter()
            .filter(|g| !g.patterns.is_empty())
            .cloned()
            .collect();
        Ok((!groups.is_empty()).then_some(Self::GitignoreV1 { groups }))
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Compiled {
    groups: Vec<Gitignore>,
}
impl Compiled {
    pub(crate) fn new(policy: Option<&Policy>, check: &impl Fn() -> Result<()>) -> Result<Self> {
        let Some(policy) = policy else {
            return Ok(Self::default());
        };
        check()?;
        let Some(policy) = policy.canonical()? else {
            return Ok(Self::default());
        };
        let mut groups = Vec::new();
        for group in policy.groups() {
            check()?;
            // An empty root and supplied lines perform no filesystem/config discovery.
            let mut builder = GitignoreBuilder::new("");
            builder.case_insensitive(group.case_insensitive)?;
            builder.allow_unclosed_class(false);
            for pattern in &group.patterns {
                check()?;
                builder.add_line(None, pattern)?;
            }
            let matcher = builder.build()?;
            if !matcher.is_empty() {
                groups.push(matcher);
            }
        }
        check()?;
        Ok(Self { groups })
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
    pub(crate) fn excludes(&self, path: &str, check: &impl Fn() -> Result<()>) -> Result<bool> {
        if self.groups.is_empty() {
            return Ok(false);
        }
        check()?;
        ensure!(
            crate::paths::normalized_syntax(path)? == path,
            "source_exclusions_path_invalid"
        );
        for group in &self.groups {
            check()?;
            // A child cannot reopen a directory excluded in this same group.
            // Each independent host layer must admit the entire parent chain.
            for (i, _) in path.match_indices('/') {
                check()?;
                if group.matched(Path::new(&path[..i]), true).is_ignore() {
                    return Ok(true);
                }
            }
            if group.matched(Path::new(path), false).is_ignore() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn null_groups<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<Vec<String>>, D::Error> {
    Ok(Option::<Vec<Vec<String>>>::deserialize(d)?.unwrap_or_default())
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectGroups {
    #[serde(
        deserialize_with = "null_groups",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub project: Vec<Vec<String>>,
    #[serde(
        deserialize_with = "null_groups",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub local: Vec<Vec<String>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostExclusions {
    #[serde(
        deserialize_with = "null_groups",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub global: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectGroups>,
}
impl HostExclusions {
    pub(crate) fn empty(&self) -> bool {
        self.global.iter().all(Vec::is_empty)
            && self
                .projects
                .values()
                .all(|p| p.project.iter().chain(&p.local).all(Vec::is_empty))
    }
    pub(crate) fn canonicalize(&mut self) {
        self.global.retain(|p| !p.is_empty());
        self.projects.retain(|_, p| {
            p.project.retain(|g| !g.is_empty());
            p.local.retain(|g| !g.is_empty());
            !p.project.is_empty() || !p.local.is_empty()
        });
    }
    pub(crate) fn select(&self, name: &str, base: Option<&Policy>) -> Result<Option<Policy>> {
        let mut groups = base.map(|p| p.groups().to_vec()).unwrap_or_default();
        let project = self.projects.get(name);
        for patterns in self.global.iter().chain(
            project
                .into_iter()
                .flat_map(|p| p.project.iter().chain(&p.local)),
        ) {
            if !patterns.is_empty() {
                groups.push(Group {
                    patterns: patterns.clone(),
                    case_insensitive: cfg!(windows),
                });
            }
        }
        Policy::GitignoreV1 { groups }.canonical()
    }
    pub(crate) fn validate(&self, host: &HostProfile, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            self.projects.len() <= 101 && self.global.len() <= MAX_GROUPS,
            "source_exclusions_configuration_budget"
        );
        for (name, p) in &self.projects {
            d.check()?;
            ensure!(
                !name.is_empty()
                    && name.len() <= 100
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
                    && (name == "default" || host.projects.iter().any(|p| p.name == *name)),
                "source_exclusions_project_not_host_registered"
            );
            ensure!(
                p.project.len() <= MAX_GROUPS && p.local.len() <= MAX_GROUPS,
                "source_exclusions_configuration_budget"
            );
        }
        for name in std::iter::once("default").chain(self.projects.keys().map(String::as_str)) {
            let policy = self.select(name, None)?;
            Compiled::new(policy.as_ref(), &|| d.check())?;
        }
        Ok(())
    }
}
pub(crate) fn unselected(value: &Option<HostExclusions>) -> bool {
    value.as_ref().is_none_or(HostExclusions::empty)
}

pub(crate) fn check_current_write_scope(
    store: &crate::store::Store,
    path: &str,
    d: &Deadline,
) -> Result<()> {
    crate::git_ignore_policy::check_current_write(store, path)?;
    #[cfg(windows)]
    if store.current_git_ignored_read_only {
        let canonical = crate::capture::canonical_write_path(store, path)?;
        crate::git_ignore_policy::check_current_write(store, &canonical)?;
    }
    let Some(policy) = &store.current_source_exclusions else {
        return Ok(());
    };
    d.check()?;
    crate::paths::normalized(path)?;
    ensure!(
        !policy.excludes(path, &|| d.check())?,
        "source_excluded_by_current_host_selection"
    );
    #[cfg(windows)]
    {
        let canonical = crate::capture::canonical_write_path(store, path)?;
        ensure!(
            !policy.excludes(&canonical, &|| d.check())?,
            "source_excluded_by_current_host_selection"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy(groups: &[&[&str]]) -> Policy {
        Policy::GitignoreV1 {
            groups: groups
                .iter()
                .map(|p| Group {
                    patterns: p.iter().map(|p| (*p).into()).collect(),
                    case_insensitive: false,
                })
                .collect(),
        }
    }
    fn matches(p: &Policy, path: &str) -> bool {
        Compiled::new(Some(p), &|| Ok(()))
            .unwrap()
            .excludes(path, &|| Ok(()))
            .unwrap()
    }
    #[test]
    fn later_groups_cannot_unignore_prior_host_rules_and_directory_parents_must_reopen() {
        let p = policy(&[&["private/**", "!private/keep.rs"], &["!private/drop.rs"]]);
        assert!(!matches(&p, "private/keep.rs"));
        assert!(matches(&p, "private/drop.rs"));
        assert!(!matches(&p, "src/main.rs"));
        assert!(matches(
            &policy(&[&["private/", "!private/keep.rs"]]),
            "private/keep.rs"
        ));
        assert!(!matches(
            &policy(&[&["private/", "!private/", "private/*", "!private/keep.rs"]]),
            "private/keep.rs"
        ));
    }
    #[test]
    fn unicode_normalization_is_not_invented_and_concrete_case_replays_independently() {
        let p = policy(&[&["src/caf\u{e9}.rs", "src/\u{1f4a1}.rs"]]);
        assert!(matches(&p, "src/caf\u{e9}.rs"));
        assert!(!matches(&p, "src/cafe\u{301}.rs"));
        assert!(matches(&p, "src/\u{1f4a1}.rs"));
        let insensitive = Policy::GitignoreV1 {
            groups: vec![Group {
                patterns: vec!["Private/**".into()],
                case_insensitive: true,
            }],
        };
        assert!(matches(&insensitive, "private/one.rs"));
        assert!(!matches(&policy(&[&["Private/**"]]), "private/one.rs"));
        let replay: Policy =
            serde_json::from_slice(&serde_json::to_vec(&insensitive).unwrap()).unwrap();
        assert!(matches(&replay, "PRIVATE/one.rs"));
    }
    #[test]
    fn malformed_patterns_paths_limits_and_expiry_refuse_instead_of_empty_matchers() {
        assert!(Compiled::new(Some(&policy(&[&["[unterminated"]])), &|| Ok(())).is_err());
        for text in ["", "a\0b", "a\rb", "a\nb"] {
            assert!(policy(&[&[text]]).canonical().is_err());
        }
        let mut p = policy(&[&["a"]]);
        let Policy::GitignoreV1 { groups } = &mut p;
        groups[0].patterns = vec!["a".repeat(256); 128];
        assert!(p.canonical().unwrap().is_some());
        let Policy::GitignoreV1 { groups } = &mut p;
        groups[0].patterns[0].push('a');
        assert!(p.canonical().is_err());
        let p = policy(&[&["private/**"]]);
        assert!(Compiled::new(Some(&p), &|| Deadline::new(0).check()).is_err());
        let matcher = Compiled::new(Some(&p), &|| Ok(())).unwrap();
        assert!(matcher.excludes("../private/one.rs", &|| Ok(())).is_err());
        assert!(
            matcher
                .excludes("private/one.rs", &|| Deadline::new(0).check())
                .is_err()
        );
    }
}
