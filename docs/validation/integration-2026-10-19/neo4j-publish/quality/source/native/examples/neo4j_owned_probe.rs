//! Explicit offline fixture/request generation. Performs no server connection.
use anyhow::{Result, ensure};
use serde_json::json;
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    neo4j_publish::{self, Plan, Target},
};
fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 2,
        "Supply target JSON and a fresh output directory"
    );
    let raw = fs::read(&args[0])?;
    let target: Target = yoke::json::decode(yoke::json::parse(&raw, 131072)?)?;
    target.validate()?;
    let output = PathBuf::from(&args[1]);
    fs::create_dir(&output)?;
    let fixture = tempfile::tempdir()?;
    fs::write(
        fixture.path().join("owned.rs"),
        "// owned \u{1f4a1}\r\nfn alpha(){beta();beta();missing();}\r\nfn beta(){alpha();}\r\n",
    )?;
    let mut engine = Engine::open(fixture.path())?;
    let d = Deadline::new(30000);
    let (snapshot, graph, _, _) = engine.index(&d)?;
    let bootstrap = Plan::bootstrap(target.clone(), &d)?;
    let publish = Plan::prepare(
        &engine.store,
        &snapshot,
        &graph,
        target,
        "absent",
        &Default::default(),
        &d,
    )?;
    for (name, plan) in [("bootstrap", bootstrap), ("publish", publish)] {
        fs::write(output.join(format!("{name}-plan.json")), plan.encode(&d)?)?;
        let requests = neo4j_publish::operations(plan.mode())
            .into_iter()
            .map(|op| neo4j_publish::request(&plan, op, &d))
            .collect::<Result<Vec<_>>>()?;
        fs::write(
            output.join(format!("{name}-requests.json")),
            serde_json::to_vec(&requests)?,
        )?;
    }
    println!(
        "{}",
        json!({"offlineFixtureGenerated":true,"networkConnections":0,"modelCalls":0,"output":output})
    );
    Ok(())
}
