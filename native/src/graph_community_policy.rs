//! Versioned numerical projection. Original directed observations stay untouched.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommunityPolicy {
    // Serde unit variants ignore extra content; an empty struct keeps the map closed.
    NeighborMajorityV1 {},
}

pub(super) struct Partition {
    pub groups: BTreeMap<usize, Vec<usize>>,
    pub excluded: BTreeSet<String>,
    pub quality: Option<f64>,
    pub details: Value,
}

struct Projection {
    neighbors: Vec<BTreeMap<usize, f64>>,
}
impl Projection {
    fn new(adjacency: &[Vec<(usize, f64)>], d: &Deadline) -> Result<Self> {
        let mut pairs = BTreeMap::<(usize, usize), Vec<f64>>::new();
        for (a, row) in adjacency.iter().enumerate() {
            for &(b, weight) in row {
                d.check()?;
                ensure!(
                    b < adjacency.len() && weight.is_finite() && weight > 0.0,
                    "Invalid positive community projection edge"
                );
                if a <= b {
                    pairs.entry((a, b)).or_default().push(weight);
                }
            }
        }
        let mut neighbors = vec![BTreeMap::new(); adjacency.len()];
        for ((a, b), mut weights) in pairs {
            d.check()?;
            // Fix the addition order independently of parallel observation order.
            weights.sort_by(f64::total_cmp);
            let weight: f64 = weights.into_iter().sum();
            ensure!(weight.is_finite(), "Community projection weight overflow");
            neighbors[a].insert(b, weight);
            if a != b {
                neighbors[b].insert(a, weight);
            }
        }
        Ok(Self { neighbors })
    }
    fn degree(&self, node: usize) -> usize {
        self.neighbors[node].len() + usize::from(self.neighbors[node].contains_key(&node))
    }
    fn cohesion(&self, members: &[usize], d: &Deadline) -> Result<f64> {
        if members.len() < 2 {
            return Ok(1.0);
        }
        let set: BTreeSet<_> = members.iter().copied().collect();
        let mut pairs = 0usize;
        for &a in members {
            for &b in self.neighbors[a].keys() {
                d.check()?;
                if a < b && set.contains(&b) {
                    pairs += 1;
                }
            }
        }
        Ok(2.0 * pairs as f64 / (members.len() * (members.len() - 1)) as f64)
    }
    fn pass(
        &self,
        members: &[usize],
        options: &AnalysisOptions,
        resolution: f64,
        calls: &mut usize,
        d: &Deadline,
    ) -> Result<(Vec<Vec<usize>>, Option<f64>)> {
        d.check()?;
        if members.is_empty() {
            return Ok((vec![], None));
        }
        let compact: BTreeMap<_, _> = members.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        let mut builder = leiden_rs::GraphDataBuilder::new(members.len());
        let mut edges = 0usize;
        for &a in members {
            for (&b, &weight) in &self.neighbors[a] {
                d.check()?;
                if a <= b
                    && let Some(&right) = compact.get(&b)
                {
                    builder.add_edge(compact[&a], right, weight)?;
                    edges += 1;
                }
            }
        }
        if edges == 0 {
            return Ok((members.iter().map(|n| vec![*n]).collect(), None));
        }
        let graph = builder.build()?;
        d.check()?;
        *calls += 1;
        let output = leiden_rs::Leiden::new(leiden_rs::LeidenConfig {
            resolution,
            seed: Some(options.seed),
            max_iterations: options.max_iterations,
            max_comm_size: options.max_community_size,
            ..Default::default()
        })
        .run(&graph)?;
        d.check()?;
        let mut covered = BTreeSet::new();
        let mut groups = Vec::new();
        for (_, local) in output.partition.communities() {
            let mut group = Vec::new();
            for index in local {
                d.check()?;
                let node = *members
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("Community member escaped projection"))?;
                ensure!(covered.insert(node), "Community member duplicated");
                group.push(node);
            }
            if !group.is_empty() {
                groups.push(group);
            }
        }
        ensure!(
            covered.len() == members.len(),
            "Community partition omitted projected nodes"
        );
        Ok((groups, output.quality.is_finite().then_some(output.quality)))
    }
    fn vote(&self, node: usize, assigned: &BTreeMap<usize, usize>) -> Option<usize> {
        let mut counts = BTreeMap::<usize, usize>::new();
        for &neighbor in self.neighbors[node].keys().filter(|n| **n != node) {
            if let Some(&community) = assigned.get(&neighbor) {
                *counts.entry(community).or_default() += 1;
            }
        }
        counts
            .into_iter()
            .max_by(|(a, av), (b, bv)| av.cmp(bv).then_with(|| b.cmp(a)))
            .map(|(c, _)| c)
    }
}

// The caller already orders nodes by source path/name/kind/position. Use that
// order rather than snapshot-scoped hash IDs, which change during location churn.
fn ordered(groups: &mut [Vec<usize>]) {
    for group in groups.iter_mut() {
        group.sort_unstable();
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
}

pub(super) fn partition(
    ids: &[&str],
    adjacency: &[Vec<(usize, f64)>],
    options: &AnalysisOptions,
    d: &Deadline,
) -> Result<Partition> {
    d.check()?;
    ensure!(
        ids.len() == adjacency.len()
            && ids.len() <= MAX_NODES
            && adjacency.iter().map(Vec::len).sum::<usize>() <= MAX_EDGES * 2
            && ids.iter().copied().collect::<BTreeSet<_>>().len() == ids.len(),
        "Invalid community projection capacity or identities"
    );
    let projection = Projection::new(adjacency, d)?;
    let mut excluded = BTreeSet::new();
    let mut threshold = None;
    if let Some(percentile) = options.exclude_hubs_percentile
        && !ids.is_empty()
    {
        let mut degrees: Vec<_> = (0..ids.len()).map(|n| projection.degree(n)).collect();
        degrees.sort();
        let at = ((ids.len() as f64 * percentile / 100.0).floor() as usize)
            .saturating_sub(1)
            .min(ids.len() - 1);
        let value = degrees[at];
        threshold = Some(value);
        excluded.extend((0..ids.len()).filter(|n| projection.degree(*n) > value));
    }
    let mut isolates = Vec::new();
    let mut stranded = Vec::new();
    let mut core = Vec::new();
    for node in 0..ids.len() {
        d.check()?;
        if excluded.contains(&node) {
            continue;
        }
        let neighbors: Vec<_> = projection.neighbors[node]
            .keys()
            .copied()
            .filter(|n| *n != node)
            .collect();
        if projection.degree(node) == 0 {
            isolates.push(node);
        } else if !neighbors.is_empty() && neighbors.iter().all(|n| excluded.contains(n)) {
            stranded.push(node);
        } else {
            core.push(node);
        }
    }
    let mut calls = 0;
    let (mut initial, quality) =
        projection.pass(&core, options, options.resolution, &mut calls, d)?;
    initial.extend(isolates.iter().map(|n| vec![*n]));
    ordered(&mut initial);
    let mut groups: BTreeMap<usize, Vec<usize>> = initial.into_iter().enumerate().collect();
    let mut assigned: BTreeMap<_, _> = groups
        .iter()
        .flat_map(|(cid, ns)| ns.iter().map(move |n| (*n, *cid)))
        .collect();
    let hubs: Vec<_> = excluded.iter().copied().collect();
    let mut next = groups.len();
    for &node in hubs.iter().chain(&stranded) {
        d.check()?;
        let chosen = projection.vote(node, &assigned);
        ensure!(
            chosen.is_some() || excluded.contains(&node),
            "Stranded community node lost its admitted hub neighbors"
        );
        let community = chosen.unwrap_or_else(|| {
            let c = next;
            next += 1;
            c
        });
        groups.entry(community).or_default().push(node);
        assigned.insert(node, community);
    }
    // Two bounded stages, each with disjoint subgraphs. A split is a new
    // Leiden pass at resolution 1, not an asserted hard maximum or recursion.
    let size_threshold = 10usize.max(ids.len() / 4);
    let mut oversized_attempts = 0usize;
    let mut sparse_attempts = 0usize;
    let mut first = Vec::new();
    for members in groups.into_values() {
        d.check()?;
        if members.len() > size_threshold {
            oversized_attempts += 1;
            first.extend(projection.pass(&members, options, 1.0, &mut calls, d)?.0);
        } else {
            first.push(members);
        }
    }
    let mut final_groups = Vec::new();
    for members in first {
        d.check()?;
        if members.len() >= 50 && projection.cohesion(&members, d)? < 0.05 {
            sparse_attempts += 1;
            final_groups.extend(projection.pass(&members, options, 1.0, &mut calls, d)?.0);
        } else {
            final_groups.push(members);
        }
    }
    ordered(&mut final_groups);
    let covered: Vec<_> = final_groups.iter().flatten().copied().collect();
    ensure!(
        covered.len() == ids.len()
            && covered.iter().copied().collect::<BTreeSet<_>>().len() == ids.len(),
        "Community policy lost or duplicated original nodes"
    );
    let groups = final_groups.into_iter().enumerate().collect();
    let excluded = hubs.iter().map(|n| ids[*n].to_owned()).collect();
    let details = json!({"kind":"neighbor_majority_v1","hubDegree":"unique_positive_weight_neighbors_self_loop_twice",
        "nodeOrdering":"native_source_path_qualified_name_kind_start_byte","hubThreshold":threshold,"hubVotes":"one_per_distinct_nonself_positive_neighbor_lowest_canonical_community_tie",
        "strandedNodes":stranded.iter().map(|n|ids[*n]).collect::<Vec<_>>(),"isolatedNodes":isolates.iter().map(|n|ids[*n]).collect::<Vec<_>>(),
        "initialCoreNodes":core.len(),"leidenCalls":calls,"qualityScope":"initial_core_before_reattachment_and_split",
        "splitStages":2,"oversizedThreshold":size_threshold,"oversizedAttempts":oversized_attempts,
        "sparseMinimumSize":50,"sparseCohesionBelow":0.05,"sparseAttempts":sparse_attempts,"splitResolution":1.0,
        "maximumSizeGuaranteed":false,"originalObservationsPreserved":true});
    d.check()?;
    Ok(Partition {
        groups,
        excluded,
        quality,
        details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options() -> AnalysisOptions {
        AnalysisOptions {
            community_policy: Some(CommunityPolicy::NeighborMajorityV1 {}),
            ..Default::default()
        }
    }
    fn graph(n: usize, edges: &[(usize, usize, f64)]) -> Vec<Vec<(usize, f64)>> {
        let mut rows = vec![vec![]; n];
        for &(a, b, weight) in edges {
            rows[a].push((b, weight));
            if a != b {
                rows[b].push((a, weight));
            }
        }
        rows
    }
    fn memberships(ids: &[&str], result: &Partition) -> BTreeSet<Vec<String>> {
        result
            .groups
            .values()
            .map(|members| {
                let mut names: Vec<_> = members.iter().map(|n| ids[*n].to_owned()).collect();
                names.sort();
                names
            })
            .collect()
    }
    #[test]
    fn distinct_neighbor_votes_hold_stranded_nodes_and_ignore_parallel_weight_bias() {
        let ids = ["a0", "a1", "a2", "b0", "b1", "hub", "tail"];
        let mut edges = vec![
            (0, 1, 10.0),
            (0, 2, 10.0),
            (1, 2, 10.0),
            (3, 4, 10.0),
            (5, 0, 0.01),
            (5, 1, 0.01),
            (5, 2, 0.01),
            (5, 3, 100.0),
            (5, 6, 1.0),
        ];
        edges.extend(std::iter::repeat_n((5, 3, 100.0), 50));
        let rows = graph(ids.len(), &edges);
        let original = rows.clone();
        let opts = AnalysisOptions {
            exclude_hubs_percentile: Some(80.0),
            ..options()
        };
        let result = partition(&ids, &rows, &opts, &Deadline::new(30000)).unwrap();
        assert_eq!(result.excluded, BTreeSet::from(["hub".to_owned()]));
        assert_eq!(result.details["strandedNodes"], json!(["tail"]));
        assert_eq!(
            memberships(&ids, &result),
            BTreeSet::from([
                vec!["a0", "a1", "a2", "hub", "tail"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                vec!["b0", "b1"].into_iter().map(str::to_owned).collect(),
            ])
        );
        assert_eq!(rows, original);
        let mut reversed = rows.clone();
        for row in &mut reversed {
            row.reverse();
        }
        let repeated = partition(&ids, &reversed, &opts, &Deadline::new(30000)).unwrap();
        assert_eq!(memberships(&ids, &result), memberships(&ids, &repeated));
        assert_eq!(result.details, repeated.details);
        let changed_snapshot_ids = ["zz", "yy", "xx", "ww", "vv", "uu", "tt"];
        let reidentified =
            partition(&changed_snapshot_ids, &rows, &opts, &Deadline::new(30000)).unwrap();
        assert_eq!(result.groups, reidentified.groups);
    }
    #[test]
    fn hub_only_neighbors_self_loops_and_true_isolates_keep_all_distinct_roles() {
        let ids = ["a", "hub", "leaf", "orphan"];
        let rows = graph(4, &[(0, 1, 1.0), (2, 1, 1.0), (1, 1, 20.0), (1, 1, 30.0)]);
        let projection = Projection::new(&rows, &Deadline::new(30000)).unwrap();
        assert_eq!(projection.degree(1), 4);
        assert_eq!(
            projection
                .cohesion(&[0, 1, 2], &Deadline::new(30000))
                .unwrap(),
            2.0 / 3.0
        );
        assert_eq!(
            projection.cohesion(&[3], &Deadline::new(30000)).unwrap(),
            1.0
        );
        let opts = AnalysisOptions {
            exclude_hubs_percentile: Some(75.0),
            ..options()
        };
        let result = partition(&ids, &rows, &opts, &Deadline::new(30000)).unwrap();
        assert_eq!(result.details["initialCoreNodes"], 0);
        assert_eq!(result.details["isolatedNodes"], json!(["orphan"]));
        assert_eq!(result.details["strandedNodes"], json!(["a", "leaf"]));
        assert_eq!(result.details["leidenCalls"], 0);
        assert!(result.quality.is_none());
        assert_eq!(
            memberships(&ids, &result),
            BTreeSet::from([
                vec!["a", "hub", "leaf"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                vec!["orphan".to_owned()],
            ])
        );
    }
    #[test]
    fn equal_neighbor_votes_use_source_order_instead_of_snapshot_hash_order() {
        let ids = ["a0", "a1", "b0", "b1", "hub", "tail"];
        let rows = graph(
            6,
            &[
                (0, 1, 10.0),
                (2, 3, 10.0),
                (4, 0, 1.0),
                (4, 2, 1.0),
                (4, 5, 1.0),
            ],
        );
        let opts = AnalysisOptions {
            exclude_hubs_percentile: Some(80.0),
            ..options()
        };
        let before = partition(&ids, &rows, &opts, &Deadline::new(30000)).unwrap();
        let changed_ids = ["zz0", "zz1", "aa0", "aa1", "hub2", "tail2"];
        let after = partition(&changed_ids, &rows, &opts, &Deadline::new(30000)).unwrap();
        assert_eq!(before.groups, after.groups);
        assert!(
            before
                .groups
                .values()
                .any(|members| members == &[0, 1, 4, 5])
        );
        assert_eq!(before.details["strandedNodes"], json!(["tail"]));
    }
    #[test]
    fn bounded_split_keeps_an_unsplittable_dense_group_without_claiming_a_maximum() {
        let names: Vec<_> = (0..24).map(|n| format!("n{n:02}")).collect();
        let ids: Vec<_> = names.iter().map(String::as_str).collect();
        let mut edges = vec![];
        for a in 0..24 {
            for b in a + 1..24 {
                edges.push((a, b, 1.0));
            }
        }
        let result =
            partition(&ids, &graph(24, &edges), &options(), &Deadline::new(30000)).unwrap();
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[&0].len(), 24);
        assert_eq!(result.details["oversizedThreshold"], 10);
        assert_eq!(result.details["oversizedAttempts"], 1);
        assert_eq!(result.details["sparseAttempts"], 0);
        assert_eq!(result.details["leidenCalls"], 2);
        assert_eq!(result.details["maximumSizeGuaranteed"], false);
    }
    #[test]
    fn sparse_stage_is_separate_bounded_and_does_not_invent_a_successful_split() {
        let names: Vec<_> = (0..61).map(|n| format!("n{n:02}")).collect();
        let ids: Vec<_> = names.iter().map(String::as_str).collect();
        let edges: Vec<_> = (1..61).map(|n| (0, n, 1.0)).collect();
        let result =
            partition(&ids, &graph(61, &edges), &options(), &Deadline::new(30000)).unwrap();
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[&0].len(), 61);
        assert_eq!(result.details["splitStages"], 2);
        assert_eq!(result.details["oversizedAttempts"], 1);
        assert_eq!(result.details["sparseAttempts"], 1);
        assert_eq!(result.details["leidenCalls"], 3);
        assert_eq!(result.details["maximumSizeGuaranteed"], false);
    }
    #[test]
    fn empty_projection_deadlines_invalid_edges_and_closed_policy_protocol_are_explicit() {
        let result = partition(&[], &[], &options(), &Deadline::new(30000)).unwrap();
        assert!(result.groups.is_empty());
        assert!(result.quality.is_none());
        assert!(partition(&[], &[], &options(), &Deadline::new(0)).is_err());
        assert!(
            partition(
                &["a", "a"],
                &[vec![], vec![]],
                &options(),
                &Deadline::new(30000)
            )
            .is_err()
        );
        for weight in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                partition(
                    &["a"],
                    &[vec![(0, weight)]],
                    &options(),
                    &Deadline::new(30000)
                )
                .is_err()
            );
        }
        assert!(
            partition(
                &["a"],
                &[vec![(99, 1.0)]],
                &options(),
                &Deadline::new(30000)
            )
            .is_err()
        );
        for policy in [
            json!({"kind":"automatic"}),
            json!({"kind":"neighbor_majority_v1","allowExecution":true}),
            json!({"kind":"neighbor_majority_v1","future":null}),
        ] {
            assert!(
                crate::json::decode::<AnalysisOptions>(json!({"communityPolicy":policy})).is_err()
            );
        }
        assert!(
            crate::json::parse(
                br#"{"communityPolicy":{"kind":"neighbor_majority_v1","kind":"automatic"}}"#,
                1024
            )
            .is_err()
        );
        let legacy = serde_json::to_value(AnalysisOptions::default()).unwrap();
        assert!(legacy.get("communityPolicy").is_none());
    }
}
