//! STAGED ONLY. Intended as a child module of host_output_recovery_tests.
//! These characterize fresh-prefix/admission authority; no cache is permitted.
use super::*;

fn two_event_scope_fixture() -> Result<(Interrupted, journal_v2::Journal)> {
    let fixture = new_fixture(SOURCE, 1)?;
    let d = Deadline::new(30_000);
    let _lock = write_state(&fixture.selection, &d)?;
    persist_plan(&fixture.selection, &fixture.plan, &d)?;
    let original = io::read_file(&fixture.target()?, MAX_FILE, false, &d)?
        .context("Scoped journal fixture original is absent")?;
    let mut journal = journal_v2::Journal {
        schema_version: 2,
        id: String::new(),
        journal_id: journal_id(&fixture.plan),
        plan_id: fixture.plan.id.clone(),
        sequence: 0,
        previous_event: None,
        phase: "preparing".into(),
        files: vec![journal_v2::Progress {
            stage: "pending".into(),
            output_identity: None,
            source_security_hash: Some(io::security_fingerprint(&original, &d)?),
            attempts: Vec::new(),
        }],
        receipt_id: None,
        result: None,
    };
    persist_constructed_v2_event(&fixture.selection, &mut journal, &d)?;
    journal.files[0].attempts.push(journal_v2::Attempt {
        ordinal: 0,
        identity: None,
        security_hash: None,
        abandoned: false,
    });
    journal.files[0].stage = "allocating".into();
    persist_constructed_v2_event(&fixture.selection, &mut journal, &d)?;
    ensure!(
        journal_v2::load(&fixture.selection, &journal.journal_id, &d)? == journal,
        "Scoped journal fixture did not validate its complete genuine prefix"
    );
    Ok((fixture, journal))
}

fn propose_registered_scope_event(journal: &journal_v2::Journal) -> journal_v2::Journal {
    let mut next = journal.clone();
    next.files[0].stage = "registered".into();
    next.files[0].attempts[0].identity = Some(format!("{:016x}:{:032x}", 0xfeed_u64, 42));
    next.files[0].attempts[0].security_hash = Some(hash("owned scoped registration fixture"));
    next
}

#[test]
fn scoped_journal_append_rechecks_an_earlier_event_after_a_successful_load() -> Result<()> {
    let (fixture, journal) = two_event_scope_fixture()?;
    let d = Deadline::new(30_000);
    let _lock = write_state(&fixture.selection, &d)?;
    let directory = state(&fixture.selection)
        .join("journals")
        .join(&journal.journal_id);
    let mut paths = names(&directory, MAX_EVENTS, &d)?;
    paths.sort();
    let first = paths.first().context("Initial scope event is absent")?;
    let mut hostile: journal_v2::Journal = metadata(first, &d)?;
    hostile.files[0].source_security_hash = Some(hash("foreign changed earlier baseline"));
    hostile.id = journal_v2::event_id(&hostile);
    // The first row's self-consistent body is changed while its durable filename
    // and the next row's predecessor retain the reviewed old identity.
    fs::write(first, crate::json::encode_bounded(&hostile, MAX_PLAN, &d)?)?;
    let before = state_manifest_with_live_lock(&fixture.selection, true)?;
    let mut proposed = propose_registered_scope_event(&journal);
    let error = journal_v2::append_event(&fixture.selection, &mut proposed, &d)
        .expect_err("A prior load must not authorize an append after prefix tampering");
    ensure!(
        format!("{error:#}").contains("Host v2 event identity/schema differs"),
        "Prefix fixture failed before its intended integrity check: {error:#}"
    );
    ensure!(
        state_manifest_with_live_lock(&fixture.selection, true)? == before
            && fs::read(fixture.target()?)? == SOURCE,
        "Refused prefix tampering changed state or source"
    );
    Ok(())
}

#[test]
fn scoped_journal_append_discovers_a_late_valid_same_sequence_fork() -> Result<()> {
    let (fixture, journal) = two_event_scope_fixture()?;
    let d = Deadline::new(30_000);
    let _lock = write_state(&fixture.selection, &d)?;
    let mut fork = journal.clone();
    // Both sequence-one alternatives are valid from the same initial row:
    // allocation intent versus rollback intent. Only their coexistence is invalid.
    fork.phase = "rolling_back".into();
    fork.files[0].stage = "pending".into();
    fork.files[0].attempts.clear();
    fork.id = journal_v2::event_id(&fork);
    let path = state(&fixture.selection)
        .join("journals")
        .join(&journal.journal_id)
        .join(format!("{:08}-{}.json", fork.sequence, fork.id));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&crate::json::encode_bounded(&fork, MAX_PLAN, &d)?)?;
    file.sync_all()?;
    drop(file);
    let before = state_manifest_with_live_lock(&fixture.selection, true)?;
    let mut proposed = propose_registered_scope_event(&journal);
    let error = journal_v2::append_event(&fixture.selection, &mut proposed, &d)
        .expect_err("A late valid same-sequence fork must be freshly discovered");
    ensure!(
        format!("{error:#}").contains("Host v2 event fork"),
        "Fork fixture failed before its intended uniqueness check: {error:#}"
    );
    ensure!(
        state_manifest_with_live_lock(&fixture.selection, true)? == before
            && fs::read(fixture.target()?)? == SOURCE,
        "Refused fork changed state or source"
    );
    Ok(())
}

#[test]
fn scoped_admission_recounts_a_new_oversized_orphan_after_prior_success() -> Result<()> {
    let (fixture, journal) = two_event_scope_fixture()?;
    let d = Deadline::new(30_000);
    let _lock = write_state(&fixture.selection, &d)?;
    let directory = state(&fixture.selection)
        .join("journals")
        .join(&journal.journal_id);
    reserve_directory(&directory, MAX_EVENTS, 1, 1, &d)?;
    let committed = names(&directory, MAX_EVENTS, &d)?
        .into_iter()
        .map(|path| -> Result<_> { Ok((path.clone(), fs::read(path)?)) })
        .collect::<Result<Vec<_>>>()?;
    let orphan = directory.join(".tmp-0123456789abcdef0123456789abcdef");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&orphan)?;
    file.set_len((MAX_STATE as u64) + 1)?;
    file.sync_all()?;
    drop(file);
    let error = reserve_directory(&directory, MAX_EVENTS, 1, 1, &d)
        .expect_err("A new oversized temporary row must exhaust fresh admission");
    ensure!(
        format!("{error:#}").contains("Host state byte capacity exceeded"),
        "Orphan fixture failed before its intended byte check: {error:#}"
    );
    ensure!(
        committed
            .iter()
            .all(|(path, bytes)| fs::read(path).is_ok_and(|actual| actual == *bytes))
            && fs::read_dir(&directory)?.count() == committed.len() + 1
            && orphan.metadata()?.len() == (MAX_STATE as u64) + 1
            && fs::read(fixture.target()?)? == SOURCE,
        "Refused fresh admission changed orphan, history or source"
    );
    // The ordinary raw_identity helper reads at most MAX_FILE bytes, so it is
    // intentionally unsuitable for this oversized admission fixture. Check
    // every expected zero byte with bounded allocation instead.
    let mut file = fs::File::open(&orphan)?;
    let mut remaining = (MAX_STATE as u64) + 1;
    let mut block = [0u8; 8192];
    while remaining > 0 {
        d.check()?;
        let count = remaining.min(block.len() as u64) as usize;
        file.read_exact(&mut block[..count])?;
        ensure!(
            block[..count].iter().all(|byte| *byte == 0),
            "Refused admission changed oversized orphan content"
        );
        remaining -= count as u64;
    }
    ensure!(
        file.read(&mut block[..1])? == 0,
        "Orphan grew after refusal"
    );
    Ok(())
}
