#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Files written before the Oracle rename (`"format": "vectorcraft"`) open, and the examples open.

fn examples() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "oracle-vector")).collect()
}

#[test]
fn the_examples_open() {
    let files = examples();
    assert!(files.len() >= 5, "{} examples", files.len());
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        oracle_vector_format::load_info(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn a_vectorcraft_file_opens_and_is_rewritten_by_the_next_save() {
    let path = examples().into_iter().find(|p| p.ends_with("bracket-drawing.oracle-vector")).unwrap();
    let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
    let old = text.replacen("\"format\": \"oracle-vector\"", "\"format\": \"vectorcraft\"", 1);
    assert_ne!(old, text, "the example is written as plain JSON");
    let (_, info) = oracle_vector_format::load_info(old.as_bytes()).unwrap();
    assert!(info.legacy && info.is_old());
    assert!(oracle_vector_format::is_native_ext("vectorcraft"));
    let wrong = text.replacen("\"format\": \"oracle-vector\"", "\"format\": \"something-else\"", 1);
    assert!(oracle_vector_format::load_info(wrong.as_bytes()).is_err());
}
