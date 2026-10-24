#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Files written before the Oracle rename (`"format": "vectorcraft"`), such as the examples, open.

#[test]
fn the_examples_open() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|e| e == "oracle-vector") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let (_, info) = oracle_vector_format::load_info(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if String::from_utf8_lossy(&bytes[..200.min(bytes.len())]).contains("vectorcraft") {
            assert!(info.legacy && info.is_old(), "{}: rewritten by the next save", path.display());
        }
        n += 1;
    }
    assert!(n >= 5, "{n} examples");
    assert!(oracle_vector_format::is_native_ext("vectorcraft"));
}
