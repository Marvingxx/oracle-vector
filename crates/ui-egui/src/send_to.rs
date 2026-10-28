//! Send to: Oracle Vector hands its document to another Oracle app (File › Send to Oracle Edit…),
//! and takes what the others send it. A document sent is placed, linked, in the receiving app's
//! open document, so a change saved here shows there (its link watch): a logo made here is a live
//! vector smart object in Oracle Edit. The exchange goes through `oracle_suite_link::inbox`: a small
//! request file the receiving app takes about every second, starting it when it isn't running.

use std::path::PathBuf;

use oracle_suite_link::App;
use oracle_suite_link::inbox::{self, Action};
use serde_json::{Value, json};

use crate::OracleVectorApp;

/// How often the inbox is looked at, in seconds.
const EVERY: f64 = 1.0;

/// The file that stands for the active document: itself when it is saved as an Oracle Vector
/// document and unchanged since, else a copy stored in the library ("Mes créations").
fn document_file(app: &mut OracleVectorApp) -> Result<PathBuf, String> {
    let st = app.session.active().ok_or("no document open")?;
    if let Some(path) = st.path.as_deref().filter(|p| p.to_ascii_lowercase().ends_with(".oracle-vector") && !st.is_dirty()) {
        return Ok(PathBuf::from(path));
    }
    crate::library_send::store_active(app)
}

/// File › Send to `target`: the document placed, linked, in `target`'s open document.
pub fn send_active(app: &mut OracleVectorApp, target: App) -> Result<Value, String> {
    let file = document_file(app)?;
    inbox::send(target, Action::Place, &file)?;
    let name = target.name();
    app.status(format!("Sent to {name}"));
    Ok(json!({"sentTo": target.id(), "path": file}))
}

/// Receive without touching the mailbox, so document creation and placement share one path.
fn receive(app: &mut OracleVectorApp, request: inbox::Request) -> Result<Value, String> {
    let path = request.file.to_string_lossy().into_owned();
    match request.action {
        Action::Place if app.session.active().is_some() => app.run("file.place", json!({"path": path})),
        Action::Place if oracle_vector_engine::oracle_files::is_foreign_picture(&path) => {
            // Validate and render before creating a document: a missing source must not leave
            // an empty document behind. Its natural picture size becomes the artboard size.
            let info = app.run("file.place.info", json!({"path": path}))?;
            app.run("file.new", json!({"width": info["width"], "height": info["height"]}))?;
            app.run("file.place", json!({"path": path}))
        }
        _ => app.run("document.open", json!({"path": path})),
    }
}

/// Takes what the other Oracle apps sent: each file placed, linked, in the active document. With
/// no document, a foreign Oracle picture gets its own artboard; other files are opened normally.
/// Looks at most every second; the watch wakes an idle app when something waits.
pub fn poll(app: &mut OracleVectorApp, ctx: &egui::Context) {
    // Once: a background thread keeps this app marked as running, and wakes it when something is sent.
    let watching = egui::Id::new("oracle-suite-inbox-watch");
    if !ctx.data(|d| d.get_temp::<bool>(watching).unwrap_or(false)) {
        ctx.data_mut(|d| d.insert_temp(watching, true));
        let waker = ctx.clone();
        inbox::watch(App::Vector, move || waker.request_repaint());
    }
    let now = ctx.input(|i| i.time);
    if now - app.inbox_last < EVERY {
        return;
    }
    app.inbox_last = now;
    let requests = inbox::take(App::Vector);
    if requests.is_empty() {
        return;
    }
    for r in requests {
        if let Err(e) = receive(app, r) {
            app.status(e);
        }
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> OracleVectorApp {
        OracleVectorApp::new(oracle_vector_engine::Session::new(), crate::Services::default())
    }

    #[test]
    fn missing_guide_does_not_create_an_empty_document() {
        let mut app = app();
        let file = std::env::temp_dir().join(format!("oracle-missing-{}.oracle-layout", std::process::id()));
        assert!(!file.exists());
        assert!(receive(&mut app, inbox::Request { action: Action::Place, file }).is_err());
        assert!(app.session.active().is_none());
    }

    #[test]
    #[ignore = "requires the Oracle Layout CLI (ORACLE_LAYOUT_CLI)"]
    fn guide_inbox_places_and_reloads_a_real_native_document() {
        let cli = App::Layout.cli().expect("set ORACLE_LAYOUT_CLI to a built Oracle Layout CLI");
        let dir = std::env::temp_dir().join(format!("oracle-layout-vector-inbox-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Charte.oracle-layout");
        let save = |width: u32| {
            let new = format!("file.new={}", json!({"width": width, "height": 120, "pages": 1, "facingPages": false, "margins": 0}));
            let output = std::process::Command::new(&cli).args(["run", "--cmd", &new, "--export"]).arg(&file).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        };
        save(180);
        let request = || inbox::Request { action: Action::Place, file: file.clone() };
        let mut empty = app();
        let before = empty.run("file.place.info", json!({"path": file})).unwrap();
        receive(&mut empty, request()).unwrap();
        let board = empty.session.active().unwrap().doc.artboards[0].rect;
        assert_eq!(board.width(), before["width"].as_f64().unwrap());
        assert_eq!(board.height(), before["height"].as_f64().unwrap());
        let image = empty.run("image.info", json!({})).unwrap();
        assert_eq!(image["linked"], true);
        assert_eq!(image["link"], json!(file));
        assert_eq!(image["pixelWidth"], before["pixelWidth"]);
        let links = empty.run("links.list", json!({})).unwrap();
        assert_eq!(links["links"][0]["status"], "ok");
        empty.run("links.updateChanged", json!({})).unwrap();
        // A second real save changes both the native file and the cached preview.
        std::thread::sleep(std::time::Duration::from_millis(30));
        save(240);
        let updated = empty.run("links.updateChanged", json!({})).unwrap();
        assert_eq!(updated["updated"].as_array().map(Vec::len), Some(1), "{updated}");
        let image = empty.run("image.info", json!({})).unwrap();
        assert!(image["pixelWidth"].as_u64().unwrap() > before["pixelWidth"].as_u64().unwrap());
        assert_eq!(image["link"], json!(file));

        let mut existing = app();
        existing.run("file.new", json!({"width": 600, "height": 400})).unwrap();
        receive(&mut existing, request()).unwrap();
        assert_eq!(existing.session.documents().len(), 1);
        let board = existing.session.active().unwrap().doc.artboards[0].rect;
        assert_eq!((board.width(), board.height()), (600.0, 400.0));
        assert_eq!(existing.run("links.list", json!({})).unwrap()["links"].as_array().unwrap().len(), 1);
        existing.run("edit.undo", json!({})).unwrap();
        assert!(existing.run("links.list", json!({})).unwrap()["links"].as_array().unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
