//! Explicit source-declaration ranking. Original graph rows and legacy policies stay intact.
use super::{
    MAX_SHORTEST_NODE_PATHS, bounded, id, identity_result, matches, path_hops, selected, text,
};
use crate::graph_view::source_claims::RawValue;
use crate::{
    capture::Deadline,
    graph_view::{
        View,
        source_claims::{self, Object},
    },
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, VecDeque},
};

#[derive(Clone, Debug)]
enum Number {
    Integer { negative: bool, digits: String },
    Float(f64),
}
fn magnitude(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}
impl Number {
    fn parse(raw: &str) -> Option<Self> {
        if raw
            .as_bytes()
            .first()
            .is_none_or(|b| !b.is_ascii_digit() && *b != b'-')
        {
            return None;
        }
        if raw.contains(['.', 'e', 'E']) {
            let number = raw.parse::<f64>().ok()?;
            return number.is_finite().then_some(Self::Float(number));
        }
        let digits = raw.strip_prefix('-').unwrap_or(raw);
        Some(Self::Integer {
            negative: raw.starts_with('-') && digits != "0",
            digits: digits.to_owned(),
        })
    }
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (
                Self::Integer {
                    negative: a,
                    digits: x,
                },
                Self::Integer {
                    negative: b,
                    digits: y,
                },
            ) => {
                if a != b {
                    return b.cmp(a);
                }
                let order = magnitude(x, y);
                if *a { order.reverse() } else { order }
            }
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(b).expect("admitted finite floats"),
            (Self::Integer { negative, digits }, Self::Float(value)) => {
                let float_negative = *value < 0.0;
                if *negative != float_negative {
                    return float_negative.cmp(negative);
                }
                // Finite binary64 integers have at most 309 decimal digits.
                // Formatting the truncated integer exactly avoids rounding the source integer.
                let absolute = value.abs();
                let integral = format!("{:.0}", absolute.trunc());
                let order = magnitude(digits, &integral).then_with(|| {
                    if absolute.fract() == 0.0 {
                        Ordering::Equal
                    } else {
                        Ordering::Less
                    }
                });
                if *negative { order.reverse() } else { order }
            }
            (Self::Float(_), Self::Integer { .. }) => other.cmp(self).reverse(),
        }
    }
}
#[derive(Clone, Debug)]
struct Rank {
    category: u8,
    score: Option<Number>,
    token: Option<String>,
}
impl Rank {
    fn unknown() -> Self {
        Self {
            category: 0,
            score: None,
            token: None,
        }
    }
    fn cmp(&self, other: &Self) -> Ordering {
        self.category
            .cmp(&other.category)
            .then_with(|| self.score.is_none().cmp(&other.score.is_none()))
            .then_with(|| match (&self.score, &other.score) {
                (Some(a), Some(b)) => a.cmp(b),
                _ => Ordering::Equal,
            })
    }
}
fn object(raw: &str, deadline: &Deadline) -> Result<Object> {
    source_claims::object(raw, deadline)
}

fn string(raw: &RawValue) -> Option<String> {
    serde_json::from_str::<String>(raw.get()).ok()
}
fn rank(data: &Object) -> Rank {
    let confidence = data.0.get("confidence").and_then(string);
    let category = match confidence.as_deref() {
        Some("EXTRACTED") => 3,
        Some("INFERRED") => 2,
        Some("AMBIGUOUS") => 1,
        _ => 0,
    };
    let token = (category != 3)
        .then(|| data.0.get("confidence_score").map(|v| v.get().to_owned()))
        .flatten();
    let score = token.as_deref().and_then(Number::parse);
    Rank {
        category,
        token: score.as_ref().and(token),
        score,
    }
}
struct Declaration {
    from: usize,
    to: usize,
    contains: bool,
    weakest: Rank,
}
#[derive(Clone)]
struct Hop {
    edge: usize,
    rank: Rank,
}

fn declaration(
    raw: Option<&RawValue>,
    a: usize,
    b: usize,
    index: &BTreeMap<&str, usize>,
    original: &Value,
    deadline: &Deadline,
) -> Result<Declaration> {
    let Some(raw) = raw else {
        return Ok(Declaration {
            from: a,
            to: b,
            contains: original["relation"] == "contains",
            weakest: Rank::unknown(),
        });
    };
    let primary = object(raw.get(), deadline)?;
    let endpoint = |key, fallback| -> Result<usize> {
        let Some(value) = primary.0.get(key) else {
            return Ok(fallback);
        };
        let key = string(value).context("Declared direction marker must name a bound endpoint")?;
        index
            .get(key.as_str())
            .copied()
            .context("Declared direction endpoint absent")
    };
    let from = endpoint("_src", a)?;
    let to = endpoint("_tgt", b)?;
    ensure!(
        (from == a && to == b) || (from == b && to == a),
        "Declared direction markers exceed original endpoint scope"
    );
    let contains = match primary.0.get("relation") {
        Some(raw) => string(raw).as_deref() == Some("contains"),
        None => original["relation"] == "contains",
    };
    let mut weakest = rank(&primary);
    if let Some(raw) = primary
        .0
        .get("also_seen")
        .filter(|r| r.get().starts_with('['))
    {
        let copies = source_claims::array(raw.get(), deadline)?;
        for copy in copies {
            deadline.check()?;
            if copy.get().starts_with('{') {
                let candidate = rank(&object(copy.get(), deadline)?);
                if candidate.cmp(&weakest) == Ordering::Less {
                    weakest = candidate;
                }
            }
        }
    }
    Ok(Declaration {
        from,
        to,
        contains,
        weakest,
    })
}
struct Key {
    rank: Rank,
    hop: usize,
}
impl Key {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank.cmp(&other.rank).then(self.hop.cmp(&other.hop))
    }
}

pub(super) fn shortest(
    store: &Store,
    view: &View,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let max = bounded(args, "max_hops", 1, 64)?;
    let claims = source_claims::selected(store, view, deadline)?;
    let sources = matches(view, text(args, "source")?, deadline)?;
    let targets = matches(view, text(args, "target")?, deadline)?;
    if sources.len() != 1 || targets.len() != 1 {
        let mut out = identity_result(view);
        out["ambiguous"] = json!(sources.len() > 1 || targets.len() > 1);
        out["found"] = json!(false);
        out["sourceCandidates"] =
            json!(sources.iter().map(|i| &view.nodes[*i]).collect::<Vec<_>>());
        out["targetCandidates"] =
            json!(targets.iter().map(|i| &view.nodes[*i]).collect::<Vec<_>>());
        return Ok(out);
    }
    let (start, end) = (sources[0], targets[0]);
    let index: BTreeMap<_, _> = view
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (id(n), i))
        .collect();
    let mut arcs = BTreeSet::new();
    let mut reverse_contains = BTreeSet::new();
    let mut originals = vec![vec![]; view.nodes.len()];
    let mut pair_originals = BTreeMap::<(usize, usize), Vec<usize>>::new();
    let mut declarations = Vec::with_capacity(view.observations.len());
    for (edge, original) in view.observations.iter().enumerate() {
        deadline.check()?;
        let mut list = Vec::new();
        if let (Some(&a), Some(&b)) = (
            index.get(original["source"].as_str().unwrap_or("")),
            index.get(original["target"].as_str().unwrap_or("")),
        ) {
            originals[a].push((b, edge));
            pair_originals.entry((a, b)).or_default().push(edge);
            if a != b {
                originals[b].push((a, edge));
                pair_originals.entry((b, a)).or_default().push(edge);
            }
            if let Some(raws) = claims.get(id(original)) {
                for raw in raws {
                    deadline.check()?;
                    list.push(declaration(Some(raw), a, b, &index, original, deadline)?);
                }
            } else {
                list.push(declaration(None, a, b, &index, original, deadline)?);
            }
            for claim in &list {
                if args["undirected"] == true {
                    arcs.insert((id(&view.nodes[a]), id(&view.nodes[b]), a, b));
                    arcs.insert((id(&view.nodes[b]), id(&view.nodes[a]), b, a));
                } else {
                    arcs.insert((
                        id(&view.nodes[claim.from]),
                        id(&view.nodes[claim.to]),
                        claim.from,
                        claim.to,
                    ));
                    if claim.contains {
                        reverse_contains.insert((
                            id(&view.nodes[claim.to]),
                            id(&view.nodes[claim.from]),
                            claim.to,
                            claim.from,
                        ));
                    }
                }
            }
        }
        declarations.push(list);
    }
    let mut adjacency = vec![vec![]; view.nodes.len()];
    let mut inserted = BTreeSet::new();
    // The retained consumer inserts sorted primary arcs first, then sorted
    // reverse-contains arcs. Sorting their union changes complete-path ties.
    for (_, _, a, b) in arcs.into_iter().chain(reverse_contains) {
        deadline.check()?;
        if inserted.insert((a, b)) {
            adjacency[a].push(b);
        }
    }
    let mut distances = BTreeMap::from([(start, 0usize)]);
    let mut predecessors = vec![vec![]; view.nodes.len()];
    let mut queue = VecDeque::from([start]);
    while let Some(at) = queue.pop_front() {
        deadline.check()?;
        let depth = distances[&at];
        if at == end || depth == max {
            continue;
        }
        for &next in &adjacency[at] {
            deadline.check()?;
            if let std::collections::btree_map::Entry::Vacant(entry) = distances.entry(next) {
                entry.insert(depth + 1);
                queue.push_back(next);
            }
            if distances[&next] == depth + 1 {
                predecessors[next].push(at);
            }
        }
    }
    // Only ancestors of the resolved target belong to a complete shortest route.
    let found = distances.contains_key(&end);
    let mut relevant = BTreeSet::new();
    let mut reverse = if found { vec![end] } else { vec![] };
    while let Some(at) = reverse.pop() {
        deadline.check()?;
        if relevant.insert(at) {
            reverse.extend(predecessors[at].iter().copied());
        }
    }
    let mut hops = BTreeMap::<(usize, usize), Hop>::new();
    for &at in &relevant {
        for &next in &adjacency[at] {
            if !relevant.contains(&next) {
                continue;
            }
            deadline.check()?;
            let Some(&depth) = distances.get(&at) else {
                continue;
            };
            if distances.get(&next).copied() != Some(depth + 1) {
                continue;
            }
            let mut forward: Option<Hop> = None;
            let mut backward: Option<Hop> = None;
            for &edge in pair_originals
                .get(&(at, next))
                .context("Declared hop originals absent")?
            {
                deadline.check()?;
                for claim in &declarations[edge] {
                    deadline.check()?;
                    let selected = if claim.from == at {
                        &mut forward
                    } else {
                        &mut backward
                    };
                    if selected
                        .as_ref()
                        .is_none_or(|old| claim.weakest.cmp(&old.rank) == Ordering::Less)
                    {
                        *selected = Some(Hop {
                            edge,
                            rank: claim.weakest.clone(),
                        });
                    }
                }
            }
            hops.insert(
                (at, next),
                forward
                    .or(backward)
                    .context("Declared route has no original hop evidence")?,
            );
        }
    }

    let mut nodes = vec![];
    let mut edges = vec![];
    let mut weakest: Option<Key> = None;
    let mut considered = 0usize;
    let mut capped = false;
    if found {
        let mut stack = vec![(end, 0usize)];
        while let Some(&(at, _)) = stack.last() {
            deadline.check()?;
            if at == start {
                if considered == MAX_SHORTEST_NODE_PATHS {
                    capped = true;
                    break;
                }
                considered += 1;
                #[cfg(test)]
                tests::observe_complete_path(considered);
                let path: Vec<_> = stack.iter().rev().map(|&(i, _)| i).collect();
                let mut key: Option<Key> = None;
                for (offset, pair) in path.windows(2).enumerate() {
                    deadline.check()?;
                    let candidate = Key {
                        rank: hops[&(pair[0], pair[1])].rank.clone(),
                        hop: offset + 1,
                    };
                    if key
                        .as_ref()
                        .is_none_or(|old| candidate.cmp(old) == Ordering::Less)
                    {
                        key = Some(candidate);
                    }
                }
                let better = nodes.is_empty()
                    || match (&key, &weakest) {
                        (None, Some(_)) => true,
                        (Some(a), Some(b)) => a.cmp(b) == Ordering::Less,
                        _ => false,
                    };
                if better {
                    edges = path.windows(2).map(|p| hops[&(p[0], p[1])].edge).collect();
                    nodes = path;
                    weakest = key;
                }
                stack.pop();
                continue;
            }
            let (_, choice) = stack.last_mut().context("Declared shortest stack absent")?;
            if *choice == predecessors[at].len() {
                stack.pop();
                continue;
            }
            let previous = predecessors[at][*choice];
            *choice += 1;
            ensure!(
                stack.len() <= 64,
                "Declared shortest stack capacity exceeded"
            );
            stack.push((previous, 0));
        }
        ensure!(
            considered > 0 && !nodes.is_empty(),
            "Declared shortest route absent"
        );
    }
    let mut out = selected(view, &nodes, &edges);
    out["found"] = json!(found);
    out["hops"] = if found {
        json!(edges.len())
    } else {
        Value::Null
    };
    out["maxHops"] = json!(max);
    out["undirected"] = args["undirected"].clone();
    out["pathObservationIds"] = json!(
        edges
            .iter()
            .map(|e| id(&view.observations[*e]))
            .collect::<Vec<_>>()
    );
    out["pathPolicy"] = json!("weakest_declared_source_claims_v1");
    out["pathSourceCategory"] =
        json!(
            weakest
                .as_ref()
                .map(|k| ["unmapped", "ambiguous", "inferred", "extracted"]
                    [usize::from(k.rank.category)])
        );
    out["pathSourceScoreToken"] = json!(weakest.as_ref().and_then(|k| k.rank.token.as_deref()));
    out["pathCategoryMeaning"] =
        json!("unverified_declared_source_claims_not_probabilities_or_execution_proof");
    out["pathHops"] = path_hops(view, &nodes, &edges, &originals, deadline)?;
    out["shortestNodePathsConsidered"] = json!(considered);
    out["shortestNodePathsCapped"] = json!(capped);
    out["shortestNodePathLimit"] = json!(MAX_SHORTEST_NODE_PATHS);
    out["pathSelectionScope"] = json!("first_256_distinct_shortest_node_paths");
    deadline.check()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering as AtomicOrdering},
        },
    };

    struct RouteProbe {
        cancel: Arc<AtomicBool>,
        completed: Rc<RefCell<Vec<usize>>>,
    }
    thread_local! {
        static ROUTE_PROBE: RefCell<Option<RouteProbe>> = const { RefCell::new(None) };
    }
    struct RestoreRouteProbe(Option<RouteProbe>);
    impl Drop for RestoreRouteProbe {
        fn drop(&mut self) {
            ROUTE_PROBE.with(|slot| slot.replace(self.0.take()));
        }
    }
    pub(super) fn observe_complete_path(considered: usize) {
        ROUTE_PROBE.with(|slot| {
            if let Some(probe) = slot.borrow().as_ref() {
                probe.completed.borrow_mut().push(considered);
                probe.cancel.store(true, AtomicOrdering::SeqCst);
            }
        });
    }

    type StoredRow = (String, String, String);
    #[derive(Debug, PartialEq, Eq)]
    struct PublishedState {
        rows: Vec<StoredRow>,
        blobs: BTreeMap<String, String>,
        latest: Option<String>,
    }
    fn published_state(store: &Store) -> PublishedState {
        let rows = store.db.prepare(
            "SELECT 'snapshot',id,value FROM snapshots UNION ALL SELECT 'graph',id,value FROM graphs UNION ALL SELECT 'record:'||kind,id,value FROM records ORDER BY 1,2,3"
        ).unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<StoredRow>>>().unwrap();
        let mut blobs = BTreeMap::new();
        for entry in std::fs::read_dir(store.dir.join("blobs")).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            let name = entry.file_name().into_string().unwrap();
            assert!(crate::model::valid_id(&name));
            let raw = std::fs::read(entry.path()).unwrap();
            assert!(raw.len() <= 2_000_000 && blobs.len() < 1000);
            assert_eq!(crate::model::hash(&raw), name);
            assert!(blobs.insert(name, crate::model::hash(&raw)).is_none());
        }
        PublishedState {
            rows,
            blobs,
            latest: store.latest().unwrap(),
        }
    }

    #[test]
    fn exact_integer_float_comparison_crosses_binary64_and_u64_boundaries() {
        for (a, b, order) in [
            (
                "18446744073709551616",
                "18446744073709551617",
                Ordering::Less,
            ),
            ("9007199254740993", "9007199254740992.0", Ordering::Greater),
            ("-9007199254740993", "-9007199254740992.0", Ordering::Less),
            ("0", "-0.0", Ordering::Equal),
            ("-0", "0.0", Ordering::Equal),
            ("1", "1.5", Ordering::Less),
            ("-1", "-1.5", Ordering::Greater),
            ("0", "-0.5", Ordering::Greater),
        ] {
            let a = Number::parse(a).unwrap();
            let b = Number::parse(b).unwrap();
            assert_eq!(a.cmp(&b), order);
            assert_eq!(b.cmp(&a), order.reverse());
        }
        for raw in ["true", "false", "null", "\"0.1\"", "1e999", "-1e999"] {
            assert!(Number::parse(raw).is_none());
        }
    }

    #[test]
    fn cancellation_after_first_real_admitted_route_stops_before_the_second_route() {
        let source = "// \u{3a9} e\u{301}\r\nfn \u{3b1}(){ stronger(); weaker(); }\r\nfn stronger(){ omega(); }\r\nfn weaker(){ omega(); }\r\nfn omega(){}\r\n";
        assert_eq!(source.len(), 111);
        assert_eq!(
            crate::model::hash(source.as_bytes()),
            "1cd640678892e21fb431a19d99b5bff3ce1b5c64b3e0828dd080169acacd4300"
        );
        let root = tempfile::TempDir::new().unwrap();
        let host = tempfile::TempDir::new().unwrap();
        std::fs::write(root.path().join("main.rs"), source).unwrap();
        let mut engine = crate::Engine::open(root.path()).unwrap();
        let deadline = Deadline::new(30000);
        let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
        let base = crate::graph_view::build(
            &engine.store,
            &snapshot,
            &graph,
            &crate::graph_view::BuildOptions {
                structural_kinds: vec![],
                ..crate::graph_view::BuildOptions::default()
            },
            &deadline,
        )
        .unwrap();
        assert_eq!(
            base.observations
                .iter()
                .filter(|e| e["relation"] == "calls")
                .count(),
            4
        );
        let raw = serde_json::to_vec(&json!({
            "schemaVersion":1,"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,
            "baseGraphHash":base.base_graph_hash,
            "claims":base.observations.iter().map(|e| json!({
                "observationId":e["id"],"source":e["source"],"target":e["target"],
                "evidenceIds":e["evidenceIds"],
                "attributes":{"confidence":"INFERRED","confidence_score":0.4},
            })).collect::<Vec<_>>(),
        }))
        .unwrap();
        let input = host.path().join("selected.json");
        std::fs::write(&input, &raw).unwrap();
        let receipt =
            source_claims::import_file(&engine.store, &snapshot, &graph, &input, &deadline)
                .unwrap();
        let view = crate::graph_view::build(
            &engine.store,
            &snapshot,
            &graph,
            &crate::graph_view::BuildOptions {
                structural_kinds: vec![],
                source_claim_overlay_ids: vec![receipt["artifactId"].as_str().unwrap().to_owned()],
                ..crate::graph_view::BuildOptions::default()
            },
            &deadline,
        )
        .unwrap();
        assert_eq!(view.nodes, base.nodes);
        assert_eq!(view.observations, base.observations);
        assert_eq!(view.evidence, base.evidence);
        let args = json!({"source":"\u{3b1}","target":"omega","max_hops":2,"undirected":false});
        let control = shortest(&engine.store, &view, &args, &deadline).unwrap();
        assert_eq!(control["found"], true);
        assert_eq!(control["hops"], 2);
        assert_eq!(control["shortestNodePathsConsidered"], 2);
        assert_eq!(control["shortestNodePathsCapped"], false);
        assert_eq!(control["pathSourceCategory"], "inferred");
        let baseline = published_state(&engine.store);
        let cancel = Arc::new(AtomicBool::new(false));
        let completed = Rc::new(RefCell::new(Vec::new()));
        let previous = ROUTE_PROBE.with(|slot| {
            slot.replace(Some(RouteProbe {
                cancel: Arc::clone(&cancel),
                completed: Rc::clone(&completed),
            }))
        });
        let restore = RestoreRouteProbe(previous);
        assert!(
            restore.0.is_none(),
            "A scoped route probe must not replace another probe"
        );
        assert!(!cancel.load(AtomicOrdering::SeqCst));
        let error = Deadline::run_bounded(&deadline, Arc::clone(&cancel), || {
            shortest(&engine.store, &view, &args, &Deadline::new(30000))
        })
        .unwrap_err();
        drop(restore);
        assert!(cancel.load(AtomicOrdering::SeqCst));
        assert_eq!(
            *completed.borrow(),
            vec![1],
            "Cancellation must be observed before visiting the second complete route"
        );
        assert!(error.to_string().contains("cancelled by host"), "{error:#}");
        engine.store.set_deadline(&Deadline::new(30000)).unwrap();
        assert_eq!(published_state(&engine.store), baseline);
        assert_eq!(
            std::fs::read(root.path().join("main.rs")).unwrap(),
            source.as_bytes()
        );
        assert_eq!(std::fs::read(input).unwrap(), raw);
    }
}
