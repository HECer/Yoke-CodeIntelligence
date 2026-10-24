use std::{collections::BTreeMap, fs, path::Path};

/// The Unix fixture explicitly permits coordination. Preserve the strict
/// Windows byte-for-byte footprint check and every original Unix file; admit
/// only newly created empty WAL / bounded shared-memory coordination files.
pub fn assert_read_footprint(root: &Path, before: BTreeMap<String, Vec<u8>>) {
    let mut after = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let bytes = fs::read(entry.path()).unwrap();
        if cfg!(unix) && !before.contains_key(&name) {
            let meta = fs::symlink_metadata(entry.path()).unwrap();
            assert!(meta.is_file() && !yoke::paths::is_link(&meta));
            match name.as_str() {
                "yoke-collection.sqlite-wal" => assert!(bytes.is_empty()),
                "yoke-collection.sqlite-shm" => assert!(bytes.len() <= 32_768),
                _ => panic!("Unexpected file created by registry read: {name}"),
            }
        } else {
            after.insert(name, bytes);
        }
    }
    assert_eq!(after, before);
}
