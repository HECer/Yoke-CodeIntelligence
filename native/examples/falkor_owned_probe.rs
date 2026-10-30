//! Explicit offline request generation for an owned provider fixture. No network.
use anyhow::{Result, ensure};
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    falkor_publish::{self as publish, Plan, Target},
};
fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 2,
        "Supply target JSON and fresh output directory"
    );
    let bytes = fs::read(&args[0])?;
    let target: Target = yoke::json::decode(yoke::json::parse(&bytes, 400000)?)?;
    target.validate()?;
    let output = PathBuf::from(&args[1]);
    fs::create_dir(&output)?;
    let source = tempfile::tempdir()?;
    let source_bytes =
        "// owned \u{1f4a1}\r\nfn alpha(){beta();beta();missing();}\r\nfn beta(){alpha();}\r\n";
    fs::write(source.path().join("owned.rs"), source_bytes)?;
    fs::write(output.join("owned.rs"), source_bytes)?;
    let mut engine = Engine::open(source.path())?;
    let d = Deadline::new(30000);
    let (s, g, _, _) = engine.index(&d)?;
    let bootstrap = Plan::bootstrap(target.clone(), &d)?;
    let publication = Plan::prepare(
        &engine.store,
        &s,
        &g,
        target,
        "absent",
        &Default::default(),
        &d,
    )?;
    for (name, plan) in [("bootstrap", bootstrap), ("publish", publication)] {
        fs::write(output.join(format!("{name}-plan.json")), plan.encode(&d)?)?;
        let commands = publish::operations(plan.mode())
            .into_iter()
            .map(|op| publish::command(&plan, op, &d))
            .collect::<Result<Vec<_>>>()?;
        fs::write(
            output.join(format!("{name}-commands.json")),
            serde_json::to_vec(&commands)?,
        )?;
    }
    println!(
        "{}",
        serde_json::json!({"offlineFixtureGenerated":true,"networkConnections":0,"modelCalls":0})
    );
    Ok(())
}
