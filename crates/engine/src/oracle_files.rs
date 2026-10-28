//! Documents of the other Oracle apps in Oracle Vector, without an export step.
//!
//! Oracle Edit `.oracle-edit` and Brand Guide `.oracle-layout` documents are placed as PNG previews
//! (the first page for a guide). The image links to the native document, so the link watch sees
//! saved changes and Update Links reads a fresh preview. The drawing is
//! `oracle_suite_link::render`, cached per version of the file.

#[cfg(not(target_arch = "wasm32"))]
use crate::EngineError;
use crate::Result;

/// Whether `name` (a path or file name) is a document another Oracle app draws as a picture for
/// Oracle Vector (Oracle Edit `.oracle-edit` / legacy `.pcraft`, or Brand Guide `.oracle-layout`).
#[cfg(not(target_arch = "wasm32"))]
pub fn is_foreign_picture(name: &str) -> bool {
    oracle_suite_link::oracle_format(std::path::Path::new(name))
        .is_some_and(|f| matches!(f.app, oracle_suite_link::App::Edit | oracle_suite_link::App::Layout))
}

#[cfg(target_arch = "wasm32")]
pub fn is_foreign_picture(_name: &str) -> bool {
    false
}

/// For a supported Oracle document at `path`: the PNG preview its app draws of it. `None` for
/// any other file (and on the web, where no app can draw).
#[cfg(not(target_arch = "wasm32"))]
pub fn foreign_picture(path: &str) -> Option<Result<Vec<u8>>> {
    if !is_foreign_picture(path) {
        return None;
    }
    let drawn = oracle_suite_link::render(std::path::Path::new(path), oracle_suite_link::Preview::Png).map_err(EngineError::Other);
    // A stale drawing (the source app is missing) still shows the picture as last drawn.
    Some(drawn.and_then(|d| std::fs::read(&d.path).map_err(|e| EngineError::Other(format!("{}: {e}", d.path.display())))))
}

#[cfg(target_arch = "wasm32")]
pub fn foreign_picture(_path: &str) -> Option<Result<Vec<u8>>> {
    None
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn guide_pictures_are_placeable_but_not_openable_as_vector_documents() {
        assert!(super::is_foreign_picture("C:/brands/Charte.ORACLE-GUIDE"));
        assert!(crate::cmd::fileio::PLACE_EXTS.contains(&"oracle-layout"));
        assert!(crate::cmd::fileio::ORACLE_EXTS.contains(&"oracle-layout"));
        assert!(!crate::cmd::fileio::OPEN_EXTS.contains(&"oracle-layout"));
        assert!(!super::is_foreign_picture("charte.pdf"));
        assert!(!super::is_foreign_picture("logo.oracle-vector"));
    }

    /// Oracle Edit saving a `w`×`h` picture of `rgb` as the `.oracle-edit` at `doc` (a real save).
    fn save_in_edit(doc: &std::path::Path, w: u32, h: u32, rgb: [u8; 3]) -> Option<()> {
        let cli = oracle_suite_link::App::Edit.cli()?;
        let png = doc.with_extension("png");
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        img.save(&png).unwrap();
        assert!(std::process::Command::new(cli).arg("convert").arg(&png).arg(doc).status().unwrap().success());
        Some(())
    }

    #[test]
    fn an_edit_document_places_linked_and_follows_its_saves() {
        assert!(super::is_foreign_picture("C:/photos/Retouche.ORACLE-EDIT"));
        assert!(super::is_foreign_picture("C:/photos/Retouche.PCRAFT"));
        assert!(!super::is_foreign_picture("logo.oracle-vector"));
        let dir = std::env::temp_dir().join(format!("oracle-vector-pcraft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("photo.oracle-edit");
        if save_in_edit(&doc, 30, 20, [211, 28, 59]).is_none() {
            return;
        }
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        s.execute("file.place", &json!({"path": doc.to_string_lossy(), "at": [150, 150]})).unwrap();
        let links = s.execute("links.list", &json!({})).unwrap();
        let link = &links["links"][0];
        assert_eq!(link["status"], "ok", "{links}");
        assert!(link["path"].as_str().unwrap().ends_with("photo.oracle-edit"), "{links}");
        // The link watch has seen the file once (the app looks every two seconds)…
        s.execute("links.updateChanged", &json!({})).unwrap();
        // …then Oracle Edit saves it again, another size: read again through a new drawing.
        std::thread::sleep(std::time::Duration::from_millis(30));
        save_in_edit(&doc, 60, 20, [28, 59, 211]).unwrap();
        let r = s.execute("links.updateChanged", &json!({})).unwrap();
        assert_eq!(r["updated"].as_array().map(Vec::len), Some(1), "{r}");
        assert_eq!(s.execute("links.list", &json!({})).unwrap()["links"][0]["status"], "ok");
    }
}
