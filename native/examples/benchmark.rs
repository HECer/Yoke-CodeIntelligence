use anyhow::Result;
use serde_json::json;
use std::{fs, time::Instant};
use yoke::{Engine, capture::Deadline};
fn main() -> Result<()> {
    let count = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "500".into())
        .parse::<usize>()?;
    anyhow::ensure!((10..=10000).contains(&count), "Expected 10..10000 files");
    let fixture = tempfile::Builder::new()
        .prefix("yoke-benchmark-")
        .tempdir()?;
    for i in 0..count {
        fs::write(
            fixture.path().join(format!("unit_{i}.rs")),
            format!(
                "// WHY: synthetic measurement fixture\npub fn work_{i}(x:u32)->u32{{ x + 1 }}\npub fn run_{i}()->u32{{work_{i}(3)}}\n"
            ),
        )?;
    }
    let mut engine = Engine::open(fixture.path())?;
    let started = Instant::now();
    let (snapshot, graph, parsed, reused) = engine.index(&Deadline::new(120000))?;
    let cold = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let (_, _, warm_parsed, warm_reused) = engine.index(&Deadline::new(120000))?;
    let warm = started.elapsed().as_secs_f64() * 1000.0;
    fs::write(
        fixture.path().join("unit_0.rs"),
        "pub fn changed(x:u32)->u32{x+2}\n",
    )?;
    let started = Instant::now();
    let (_, _, change_parsed, change_reused) = engine.index(&Deadline::new(120000))?;
    let change = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let found = engine.invoke(
        "ci_find",
        json!({"snapshotId":snapshot.id,"query":"work_123","limit":5}),
    )?;
    let query = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{}",
        json!({"kind":"synthetic-native-microbenchmark","version":env!("CARGO_PKG_VERSION"),"files":count,"entities":graph.entities.len(),"observations":graph.observations.len(),"coldMs":cold,"unchangedStrictCaptureMs":warm,"oneFileChangeMs":change,"explicitSnapshotQueryMs":query,"cold":{"parsed":parsed,"reused":reused},"unchanged":{"parsed":warm_parsed,"reused":warm_reused},"oneFileChange":{"parsed":change_parsed,"reused":change_reused},"queryStatus":found["status"],"limitations":["One synthetic corpus and one process; no upstream comparison, whole-agent token/cost measurement or cross-platform performance claim."]})
    );
    Ok(())
}
