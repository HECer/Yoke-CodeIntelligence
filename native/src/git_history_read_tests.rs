use super::{History, Objects, Options, walk};
use crate::capture::Deadline;
use anyhow::Result;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct PendingRead {
    started: mpsc::Sender<()>,
    cancelled: mpsc::Receiver<()>,
}
struct Reader {
    objects: BTreeMap<String, Vec<u8>>,
    ceiling: Deadline,
    delay: Duration,
    pending: Option<PendingRead>,
    sizes: usize,
    bodies: usize,
    exceeded_ceiling: bool,
}
impl Reader {
    fn observe(&mut self, d: &Deadline) {
        // Read the outer ceiling first so scheduling between these reads cannot
        // falsely classify the same Deadline as a newly extended deadline.
        let maximum = self
            .ceiling
            .remaining()
            .saturating_add(Duration::from_millis(5));
        self.exceeded_ceiling |= d.remaining() > maximum;
    }
}
impl Objects for Reader {
    fn size(&mut self, id: &str, d: &Deadline) -> Result<usize> {
        self.observe(d);
        self.sizes += 1;
        std::thread::sleep(self.delay);
        d.check()?;
        self.objects
            .get(id)
            .map(Vec::len)
            .ok_or_else(|| anyhow::anyhow!("Poisoned ancestor must remain unread"))
    }
    fn contents(&mut self, id: &str, _maximum: usize, d: &Deadline) -> Result<Vec<u8>> {
        self.observe(d);
        self.bodies += 1;
        if let Some(pending) = self.pending.take() {
            pending.started.send(())?;
            pending.cancelled.recv_timeout(Duration::from_secs(1))?;
            // Intentionally return a successful body after cancellation. The
            // traversal must observe the host signal before another object.
        } else {
            std::thread::sleep(self.delay);
            d.check()?;
        }
        Ok(self.objects[id].clone())
    }
}

#[test]
fn information_capacity_is_checked_before_requesting_any_commit_contents() {
    let d = Deadline::new(2000);
    let (mut reader, head) = fixture(&d);
    let options = Options {
        max_commit_bytes: reader.objects[&head].len() - 1,
        ..Options::default()
    };
    let error = walk(&mut reader, &head, &options, &d).unwrap_err();
    assert!(error.to_string().contains("capacity"));
    assert_eq!((reader.sizes, reader.bodies), (1, 0));
}

#[test]
fn selection_and_visit_boundaries_never_probe_the_poisoned_ancestor() {
    for limit_visits in [false, true] {
        let d = Deadline::new(2000);
        let (mut reader, head) = fixture(&d);
        reader.objects.retain(|id, _| id == &head);
        let mut options = Options::default();
        if limit_visits {
            options.max_visits = 1;
        } else {
            options.max_commits = 1;
        }
        let history = walk(&mut reader, &head, &options, &d).unwrap();
        assert_eq!(history.commits.len(), 1);
        assert_eq!(history.summary["frontierCommits"], 1);
        assert_eq!((reader.sizes, reader.bodies), (1, 1));
    }
}

#[test]
fn partial_date_frontier_cannot_supply_a_fabricated_selection_under_the_visit_limit() {
    let d = Deadline::new(2000);
    let (mut reader, root) = fixture(&d);
    let tree = "a".repeat(40);
    let left = format!("tree {tree}\nparent {root}\nauthor F <f@example.invalid> 9 +0000\ncommitter F <f@example.invalid> 2 +0000\n\nleft\n").into_bytes();
    let left_id = super::super::object_id("commit", &left, 40);
    let unread_right = "b".repeat(40);
    let head = format!("tree {tree}\nparent {left_id}\nparent {unread_right}\nauthor F <f@example.invalid> 1 +0000\ncommitter F <f@example.invalid> 3 +0000\n\nmerge\n").into_bytes();
    let head_id = super::super::object_id("commit", &head, 40);
    reader.objects.insert(left_id, left);
    reader.objects.insert(head_id.clone(), head);
    let options = Options {
        max_visits: 2,
        ..Options::default()
    };
    let history = walk(&mut reader, &head_id, &options, &d).unwrap();
    assert!(
        history.commits.is_empty(),
        "The unread competitor may have a later date"
    );
    assert_eq!((reader.sizes, reader.bodies), (2, 2));
    assert_eq!(history.summary["objectsRead"], 2);
    assert_eq!(history.summary["visitedCommits"], 1);
    assert_eq!(history.summary["frontierCommits"], 2);
    assert_eq!(history.summary["truncationReason"], "visit_limit");
}

#[test]
fn misplaced_parent_headers_refuse_before_acquiring_unrelated_history() {
    for intervening in [
        "author Fixture <fixture@example.invalid> 2 +0000\n",
        "encoding UTF-8\n",
        "x-extra metadata\n",
        "gpgsig signed\n continuation\n",
    ] {
        let d = Deadline::new(2000);
        let (mut reader, _) = fixture(&d);
        // Hash-valid bytes are not sufficient ancestry evidence. This object
        // deliberately puts a canonical parent after the parent-header phase.
        let unrelated = "b".repeat(40);
        let author = if intervening.starts_with("author ") {
            ""
        } else {
            "author Fixture <fixture@example.invalid> 2 +0000\n"
        };
        let raw = format!(
            "tree {}\n{intervening}parent {unrelated}\n{author}committer Fixture <fixture@example.invalid> 2 +0000\n\nINPUT_BODY_CANARY\n",
            "a".repeat(40)
        ).into_bytes();
        let head = super::super::object_id("commit", &raw, 40);
        reader.objects = BTreeMap::from([(head.clone(), raw)]);
        let error = walk(&mut reader, &head, &Options::default(), &d).unwrap_err();
        assert_eq!(error.to_string(), "github_app_digest_history_unavailable");
        assert_eq!((reader.sizes, reader.bodies), (1, 1));
    }
}
fn fixture(d: &Deadline) -> (Reader, String) {
    let tree = "a".repeat(40);
    let root = format!("tree {tree}\nauthor Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\nroot\n").into_bytes();
    let root_id = super::super::object_id("commit", &root, 40);
    let head = format!("tree {tree}\nparent {root_id}\nauthor Fixture <fixture@example.invalid> 2 +0000\ncommitter Fixture <fixture@example.invalid> 2 +0000\n\nhead\n").into_bytes();
    let head_id = super::super::object_id("commit", &head, 40);
    (
        Reader {
            objects: BTreeMap::from([(root_id, root), (head_id.clone(), head)]),
            ceiling: d.clone(),
            delay: Duration::ZERO,
            pending: None,
            sizes: 0,
            bodies: 0,
            exceeded_ceiling: false,
        },
        head_id,
    )
}

#[test]
fn object_information_and_contents_share_the_original_deadline() {
    let d = Deadline::new(65);
    let (mut reader, head) = fixture(&d);
    reader.delay = Duration::from_millis(40);
    let start = Instant::now();
    let result: Result<History> = walk(&mut reader, &head, &Options::default(), &d);
    assert!(result.unwrap_err().to_string().contains("deadline"));
    assert!(
        !reader.exceeded_ceiling,
        "An object request extended the host deadline"
    );
    assert_eq!((reader.sizes, reader.bodies), (1, 1));
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[test]
fn cancellation_during_contents_prevents_another_object_read() {
    let flag = Arc::new(AtomicBool::new(false));
    let mut observed = None;
    let result = Deadline::run_bounded(&Deadline::new(2000), Arc::clone(&flag), || {
        let d = Deadline::new(2000);
        let (mut reader, head) = fixture(&d);
        let (started, wait_started) = mpsc::channel();
        let (cancelled, wait_cancelled) = mpsc::channel();
        reader.pending = Some(PendingRead {
            started,
            cancelled: wait_cancelled,
        });
        let worker = std::thread::spawn(move || {
            if wait_started
                .recv_timeout(Duration::from_millis(100))
                .is_ok()
            {
                flag.store(true, Ordering::SeqCst);
                let _ = cancelled.send(());
                true
            } else {
                false
            }
        });
        let result = walk(&mut reader, &head, &Options::default(), &d);
        // Capture the inner outcome before joining or leaving run_bounded.
        observed = Some((
            result.err().map(|e| e.to_string()),
            d.check().is_err(),
            reader.sizes,
            reader.bodies,
            reader.exceeded_ceiling,
        ));
        let requested = worker.join().unwrap();
        assert!(requested, "Traversal never entered the pending object read");
        Ok(())
    });
    let (error, cancelled_at_return, sizes, bodies, exceeded_ceiling) = observed.unwrap();
    assert!(cancelled_at_return && error.is_some_and(|error| error.contains("cancelled")));
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!((sizes, bodies), (1, 1));
    assert!(!exceeded_ceiling);
}
