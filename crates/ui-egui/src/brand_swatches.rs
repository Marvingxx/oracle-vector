//! The brands of Oracle Brand Kit as swatch libraries (Window › Swatch Libraries › Brands), kept as
//! the brands are: every two seconds the brand registry's stamp is looked at (a few file times,
//! `oracle_suite_link::brands`); a change reads the brands again. The active document's brand
//! colour groups are then brought up to date (`swatch.library.updateBrand`), and so is a document
//! when it becomes the active one. Desktop app only (it has a user library folder; tests don't).

use oracle_vector_engine::cmd::swatchlib::{brand_library, libraries};
use serde_json::json;

use crate::OracleVectorApp;

/// How often the brands are looked at, in seconds.
const EVERY: f64 = 2.0;

#[derive(Clone, Copy)]
struct Seen {
    at: f64,
    stamp: u64,
    /// The document whose brand groups were last brought up to date, and at which stamp.
    doc: Option<(u64, u64)>,
}

pub fn poll(app: &mut OracleVectorApp, ctx: &egui::Context) {
    if cfg!(test) || app.session.swatch_libraries.user_dir().is_none() {
        return;
    }
    let id = egui::Id::new("oracle-brand-swatches");
    let now = ctx.input(|i| i.time);
    let seen = ctx.data(|d| d.get_temp::<Seen>(id));
    if seen.is_some_and(|s| now - s.at < EVERY) {
        return;
    }
    let stamp = oracle_suite_link::brands::stamp();
    let mut next = Seen { at: now, stamp, doc: seen.and_then(|s| s.doc) };
    if seen.is_none_or(|s| s.stamp != stamp) {
        let brands = oracle_suite_link::brands::brands()
            .into_iter()
            .map(|b| {
                let colors: Vec<(String, String)> = b.colors.iter().map(|c| (c.name.clone(), c.hex.clone())).collect();
                (b.id.clone(), brand_library(&b.name, &colors))
            })
            .collect();
        if app.session.swatch_libraries.set_brands(brands) {
            ctx.request_repaint();
        }
    }
    let doc = app.session.active().map(|d| d.uid);
    if let Some(uid) = doc
        && next.doc != Some((uid, stamp))
    {
        next.doc = Some((uid, stamp));
        let brand_libraries: Vec<String> = libraries(&app.session).into_iter().filter(|l| l.category == "brand").map(|l| l.id).collect();
        for library in brand_libraries {
            match app.run("swatch.library.updateBrand", json!({ "library": library })) {
                Ok(r) if r["updated"].as_array().is_some_and(|a| !a.is_empty()) || r["added"].as_array().is_some_and(|a| !a.is_empty()) => {
                    ctx.request_repaint();
                }
                Ok(_) => {}
                Err(e) => log::info!("brand colours not brought up to date: {e}"),
            }
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, next));
}
