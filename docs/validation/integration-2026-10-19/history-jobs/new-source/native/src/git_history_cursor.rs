//! Replayable date-priority traversal. Persist proofs and visit order, never a
//! caller-supplied ready queue or claimed exhaustion flag.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub objects: usize,
    pub raw_bytes: usize,
    pub parent_ids: usize,
    pub frontier: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            objects: 16_000,
            raw_bytes: 16_000_000,
            parent_ids: 64_000,
            frontier: 4_000,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        let hard = Self::default();
        ensure!(
            self.objects > 0
                && self.objects <= hard.objects
                && self.raw_bytes <= hard.raw_bytes
                && self.parent_ids <= hard.parent_ids
                && self.frontier > 0
                && self.frontier <= hard.frontier,
            Capacity
        );
        Ok(())
    }
}
/// Only raw object bodies and the visit prefix are durable. All other state is
/// reconstructed under the original head's Merkle parent closure.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Proof {
    pub objects: BTreeMap<String, String>,
    pub visited: Vec<String>,
}
pub(crate) struct Cursor {
    head: String,
    limits: Limits,
    pending: VecDeque<(String, usize)>,
    ready: BTreeMap<(Reverse<u64>, usize), (String, Commit)>,
    seen: BTreeSet<String>,
    loaded: BTreeSet<String>,
    pub raw_bytes: usize,
    pub parent_ids: usize,
}
impl Cursor {
    pub fn replay(head: &str, proof: &Proof, limits: &Limits, d: &Deadline) -> Result<Self> {
        d.check()?;
        limits.validate()?;
        ensure!(
            super::super::oid_valid(head)
                && proof.objects.len() <= limits.objects
                && proof.visited.len() <= proof.objects.len(),
            UNAVAILABLE
        );
        let mut cursor = Self {
            head: head.into(),
            limits: limits.clone(),
            pending: VecDeque::from([(head.into(), 0)]),
            ready: BTreeMap::new(),
            seen: BTreeSet::from([head.into()]),
            loaded: BTreeSet::new(),
            raw_bytes: 0,
            parent_ids: 0,
        };
        for expected in &proof.visited {
            cursor.hydrate_known(proof, d)?;
            ensure!(cursor.pending.is_empty(), UNAVAILABLE);
            let (id, _) = cursor.visit(d)?.context(UNAVAILABLE)?;
            ensure!(&id == expected, UNAVAILABLE);
        }
        cursor.hydrate_known(proof, d)?;
        // Refuse unrelated, undiscovered or redundant proofs; imported object
        // labels cannot make progress, change ordering or pin local CAS.
        ensure!(cursor.loaded.len() == proof.objects.len(), UNAVAILABLE);
        Ok(cursor)
    }
    fn hydrate_known(&mut self, proof: &Proof, d: &Deadline) -> Result<()> {
        let mut missing = VecDeque::new();
        while let Some((id, ordinal)) = self.pending.pop_front() {
            d.check()?;
            if let Some(raw) = proof.objects.get(&id) {
                self.admit(&id, ordinal, raw, d)?;
            } else {
                missing.push_back((id, ordinal));
            }
        }
        self.pending = missing;
        Ok(())
    }
    fn admit(&mut self, id: &str, ordinal: usize, raw: &str, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            raw.len() <= 1_000_000
                && raw.len() <= self.limits.raw_bytes.saturating_sub(self.raw_bytes)
                && self.loaded.len() < self.limits.objects,
            Capacity
        );
        ensure!(
            id.len() == self.head.len()
                && super::super::object_id("commit", raw.as_bytes(), id.len()) == id
                && !crate::paths::obvious_secret(raw),
            UNAVAILABLE
        );
        let mut remaining = self.limits.parent_ids - self.parent_ids;
        let parsed = parse(raw.as_bytes(), self.head.len(), &mut remaining, d)?;
        self.parent_ids = self.limits.parent_ids - remaining;
        self.raw_bytes += raw.len();
        ensure!(self.loaded.insert(id.into()), UNAVAILABLE);
        ensure!(
            self.ready
                .insert(
                    (Reverse(parsed.committer_timestamp), ordinal),
                    (id.into(), parsed)
                )
                .is_none(),
            UNAVAILABLE
        );
        Ok(())
    }
    pub fn next_pending(&self) -> Option<&str> {
        self.pending.front().map(|(id, _)| id.as_str())
    }
    /// Size admission precedes cat-file body capture. The raw body is retained
    /// only after strict protocol, object hash, UTF-8 and secret-policy checks.
    pub fn read_pending(
        &mut self,
        git: &super::super::HostGit,
        root: &Path,
        proof: &mut Proof,
        max_bytes: usize,
        d: &Deadline,
    ) -> Result<Option<usize>> {
        d.check()?;
        let (id, ordinal) = self.pending.front().cloned().context(UNAVAILABLE)?;
        ensure!(
            root.is_absolute() && !proof.objects.contains_key(&id),
            UNAVAILABLE
        );
        let root = root.canonicalize().context(UNAVAILABLE)?;
        let mut objects = GitObjects { git, root: &root };
        let size = objects.size(&id, d)?;
        ensure!(
            size <= 1_000_000
                && size <= self.limits.raw_bytes.saturating_sub(self.raw_bytes)
                && self.loaded.len() < self.limits.objects,
            Capacity
        );
        if size > max_bytes {
            return Ok(None);
        }
        let bytes = objects.contents(&id, size, d)?;
        ensure!(bytes.len() == size, UNAVAILABLE);
        let raw = std::str::from_utf8(&bytes).context(UNAVAILABLE)?;
        self.admit(&id, ordinal, raw, d)?;
        self.pending.pop_front();
        proof.objects.insert(id, raw.into());
        Ok(Some(size))
    }
    pub fn exhausted(&self) -> bool {
        self.pending.is_empty() && self.ready.is_empty()
    }
    pub fn frontier_len(&self) -> usize {
        self.pending.len() + self.ready.len()
    }
    pub fn missing_len(&self) -> usize {
        self.pending.len()
    }
    pub fn visit(&mut self, d: &Deadline) -> Result<Option<(String, Option<Value>)>> {
        d.check()?;
        ensure!(self.pending.is_empty(), UNAVAILABLE);
        let Some((_, (id, parsed))) = self.ready.pop_first() else {
            return Ok(None);
        };
        for parent in &parsed.parents {
            d.check()?;
            if !self.seen.contains(parent) {
                ensure!(self.frontier_len() < self.limits.frontier, Capacity);
                let ordinal = self.seen.len();
                self.seen.insert(parent.clone());
                self.pending.push_back((parent.clone(), ordinal));
            }
        }
        let row = (parsed.parents.len() <= 1 && !parsed.subject.is_empty()).then(|| {
            json!({"sha":id,"subject":parsed.subject,"body":parsed.body,
                "parentIds":parsed.parents,"objectVerified":true,
                "files":[],"filesStatus":"unavailable","symbols":[],"symbolsStatus":"unavailable"})
        });
        Ok(Some((id, row)))
    }
    pub fn required_captures(head: &str, proof: &Proof, d: &Deadline) -> Result<BTreeSet<String>> {
        let mut ids = BTreeSet::new();
        let mut parents = Limits::default().parent_ids;
        for id in &proof.visited {
            d.check()?;
            let raw = proof.objects.get(id).context(UNAVAILABLE)?;
            let commit = parse(raw.as_bytes(), head.len(), &mut parents, d)?;
            if commit.parents.len() <= 1 && !commit.subject.is_empty() {
                ids.insert(id.clone());
                ids.extend(commit.parents);
            }
        }
        ids.remove(head);
        Ok(ids)
    }
}
