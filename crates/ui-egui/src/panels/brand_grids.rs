//! Brand Grids panel: base, construction and clearspace grids, lockups and bento layouts
//! (`oracle-brandtools`, through the engine's `brand.*` commands). Each tab keeps its settings; with
//! Live Update on, changing one redraws the grid that tab made last, in place, once the mouse is
//! released, so an adjustment is one undo step.

use egui::Ui;
use oracle_brandtools as bt;
use serde_json::{Value, json};

use super::{pstate, set_pstate};
use crate::OracleVectorApp;
use crate::widgets;

pub const ID: &str = "brandGrids";
const KEY: &str = "brand-grids";
const TABS: [&str; 5] = ["Base", "Construction", "Clearspace", "Lockups", "Bento"];

#[derive(Clone, PartialEq)]
struct State {
    tab: usize,
    base: bt::BaseParams,
    construction: bt::ConstructionParams,
    clearspace: bt::ClearspaceParams,
    lockup: bt::LockupParams,
    bento: bt::BentoParams,
    /// The layer each grid tab made last (Base, Construction, Clearspace, Lockups).
    layers: [Option<u64>; 4],
    live: bool,
    /// Settings changed since the grid was drawn.
    dirty: bool,
    message: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            tab: 1,
            base: bt::BaseParams::default(),
            construction: bt::ConstructionParams::default(),
            clearspace: bt::ClearspaceParams::default(),
            lockup: bt::LockupParams::default(),
            bento: bt::BentoParams::default(),
            layers: [None; 4],
            live: true,
            dirty: false,
            message: None,
        }
    }
}

/// A dropdown over `(label, value)` choices; true when the value changed.
fn choose<T: Copy + PartialEq>(ui: &mut Ui, id: &str, label: &str, value: &mut T, options: &[(&str, T)]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        widgets::dim_label(ui, tl!(label));
        let current = options.iter().find(|(_, v)| v == value).map_or("", |(l, _)| tl!(l));
        let labels: Vec<&str> = options.iter().map(|(l, _)| tl!(l)).collect();
        if let Some(i) = widgets::dropdown(ui, ("brand-grids", id), current, &labels, 150.0)
            && let Some((_, v)) = options.get(i)
        {
            changed = *v != *value;
            *value = *v;
        }
    });
    changed
}

fn slide(ui: &mut Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str) {
    ui.horizontal(|ui| {
        widgets::dim_label(ui, tl!(label));
        widgets::slider(ui, egui::Slider::new(value, range).suffix(suffix).max_decimals(2));
    });
}

fn tick(ui: &mut Ui, label: &str, value: &mut bool) {
    if widgets::check(ui, tl!(label), *value, true) {
        *value = !*value;
    }
}

fn base_ui(ui: &mut Ui, p: &mut bt::BaseParams) {
    use bt::BaseKind::*;
    choose(ui, "kind", "Kind", &mut p.kind, &[("Square", Square), ("Isometric", Isometric), ("Hexagon", Hexagon), ("Golden Ratio", GoldenRatio)]);
    if p.kind == GoldenRatio {
        let mut steps = p.golden_steps as f64;
        slide(ui, "Steps", &mut steps, 1.0..=12.0, "");
        p.golden_steps = steps.round() as usize;
    } else {
        slide(ui, "Size", &mut p.size, 2.0..=300.0, " pt");
    }
    slide(ui, "Stroke", &mut p.stroke, 0.1..=5.0, " pt");
}

fn construction_ui(ui: &mut Ui, p: &mut bt::ConstructionParams) {
    egui::Grid::new("brand-grids-elements").num_columns(2).show(ui, |ui| {
        tick(ui, "Horizontals", &mut p.horizontals);
        tick(ui, "Verticals", &mut p.verticals);
        ui.end_row();
        tick(ui, "Diagonals", &mut p.diagonals);
        tick(ui, "Circles", &mut p.circles);
        ui.end_row();
        tick(ui, "Anchors", &mut p.anchors);
        tick(ui, "Handles", &mut p.handles);
        ui.end_row();
        tick(ui, "Outlines", &mut p.outlines);
        tick(ui, "Spacing", &mut p.spacing);
        ui.end_row();
        tick(ui, "Measurements", &mut p.measurements);
        tick(ui, "Clean", &mut p.clean);
        ui.end_row();
    });
    widgets::divider(ui);
    let mut percent = p.sensitivity * 100.0;
    slide(ui, "Sensitivity", &mut percent, 0.05..=3.0, " %");
    p.sensitivity = percent / 100.0;
    slide(ui, "Angle Tolerance", &mut p.angle_tolerance_deg, 0.0..=15.0, "°");
    use bt::CircleFilter::*;
    choose(ui, "circles", "Circle Size", &mut p.circle_filter, &[("All", All), ("Big", Big), ("Small", Small)]);
    ui.add_enabled_ui(p.spacing, |ui| {
        use bt::Placement::*;
        choose(ui, "spacing", "Spacing Markers", &mut p.spacing_placement, &[("Inside", Inside), ("Outside", Outside)]);
    });
    use bt::Extent::*;
    choose(ui, "extent", "Extent", &mut p.extent, &[("Logo", Logo), ("Artboard", Artboard)]);
}

fn clearspace_ui(ui: &mut Ui, p: &mut bt::ClearspaceParams) {
    use bt::ClearUnit::*;
    let before = p.unit;
    choose(ui, "unit", "Unit", &mut p.unit, &[("Logomark", Logomark), ("Pixels", Px), ("Centimeters", Cm), ("Inches", In)]);
    if p.unit != before {
        p.size = match p.unit {
            Logomark => 1.0,
            Px => 24.0,
            Cm => 1.0,
            In => 0.5,
        };
    }
    let (range, suffix) = match p.unit {
        Logomark => (0.1..=3.0, " ×"),
        Px => (1.0..=500.0, " px"),
        Cm => (0.1..=20.0, " cm"),
        In => (0.05..=8.0, " in"),
    };
    slide(ui, "Size", &mut p.size, range, suffix);
    tick(ui, "Invert Components", &mut p.invert);
    tick(ui, "Fill Boxes", &mut p.fill);
    slide(ui, "Min. Print Size", &mut p.min_print_mm, 0.0..=100.0, " mm");
    slide(ui, "Min. Screen Size", &mut p.min_screen_px, 0.0..=500.0, " px");
}

fn lockup_ui(ui: &mut Ui, p: &mut bt::LockupParams) {
    use bt::LockupKind::*;
    choose(ui, "lockup", "Kind", &mut p.kind, &[("Horizontal", Horizontal), ("Vertical", Vertical), ("Condensed", Condensed), ("Partner", Partner)]);
    slide(ui, "Spacing", &mut p.spacing, 0.0..=3.0, " ×");
    tick(ui, "Invert Components", &mut p.invert);
}

fn bento_ui(ui: &mut Ui, p: &mut bt::BentoParams) {
    use bt::BentoStyle::*;
    choose(ui, "bento", "Style", &mut p.style, &[("Simple", Simple), ("Centered", Centered), ("Brick", Brick), ("Random", Random)]);
    let mut sections = p.sections as f64;
    slide(ui, "Sections", &mut sections, 1.0..=24.0, "");
    p.sections = sections.round() as usize;
    slide(ui, "Corner Radius", &mut p.radius, 0.0..=100.0, " pt");
    slide(ui, "Gutter", &mut p.gutter, 0.0..=100.0, " pt");
    tick(ui, "Numbers", &mut p.numbers);
}

/// The command a tab runs and its parameters.
fn request(st: &State, replace: bool) -> (&'static str, Value) {
    let layer = if replace { st.layers.get(st.tab).copied().flatten() } else { None };
    let (id, params) = match st.tab {
        0 => ("brand.baseGrid", serde_json::to_value(&st.base)),
        1 => ("brand.constructionGrid", serde_json::to_value(&st.construction)),
        2 => ("brand.clearspace", serde_json::to_value(&st.clearspace)),
        3 => ("brand.lockup", serde_json::to_value(&st.lockup)),
        _ => ("brand.bento", serde_json::to_value(&st.bento)),
    };
    (id, json!({ "params": params.unwrap_or_default(), "replace": layer }))
}

/// Whether the selection is one bento grid this panel made (a live change may redraw it).
fn bento_selected(app: &OracleVectorApp) -> bool {
    let Ok(st) = app.session.doc() else { return false };
    let ids = st.selection.in_paint_order(&st.doc);
    matches!(ids.as_slice(), [id] if st.doc.node(*id).is_some_and(|n| n.name.as_deref() == Some("Bento")))
}

fn run(app: &mut OracleVectorApp, st: &mut State, replace: bool) {
    let (id, params) = request(st, replace);
    match app.run(id, params) {
        Ok(r) => {
            if let Some(slot) = st.layers.get_mut(st.tab) {
                *slot = r["layer"].as_u64();
            }
            st.message = None;
        }
        Err(e) => st.message = Some(e.to_string()),
    }
    st.dirty = false;
}

pub fn show(app: &mut OracleVectorApp, ui: &mut Ui) {
    let mut st: State = pstate(ui.ctx(), KEY);
    let before = st.clone();
    let tabs: Vec<&str> = TABS.iter().map(|t| tl!(t)).collect();
    ui.horizontal_wrapped(|ui| {
        for (i, t) in tabs.iter().enumerate() {
            if ui.selectable_label(st.tab == i, *t).clicked() {
                st.tab = i;
            }
        }
    });
    widgets::divider(ui);
    match st.tab {
        0 => base_ui(ui, &mut st.base),
        1 => construction_ui(ui, &mut st.construction),
        2 => clearspace_ui(ui, &mut st.clearspace),
        3 => lockup_ui(ui, &mut st.lockup),
        _ => bento_ui(ui, &mut st.bento),
    }
    let settings_changed = st.tab == before.tab
        && (st.base != before.base
            || st.construction != before.construction
            || st.clearspace != before.clearspace
            || st.lockup != before.lockup
            || st.bento != before.bento);
    if settings_changed {
        st.dirty = true;
    }
    widgets::divider(ui);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, tl!("Create")).clicked() {
            run(app, &mut st, false);
        }
        if st.tab == 0 && st.base.kind == bt::BaseKind::Square && ui.button(tl!("Make Guides")).clicked() {
            let (_, mut params) = request(&st, false);
            params["makeGuides"] = json!(true);
            if let Err(e) = app.run("brand.baseGrid", params) {
                st.message = Some(e.to_string());
            }
        }
        if st.tab == 4 && ui.button(tl!("Shuffle")).clicked() {
            st.bento.seed = st.bento.seed.wrapping_add(1);
            run(app, &mut st, false);
        }
    });
    tick(ui, "Live Update", &mut st.live);
    // Redraw once the mouse is up, so a drag is one step.
    if st.live && st.dirty && !ui.input(|i| i.pointer.any_down()) {
        let made = st.layers.get(st.tab).copied().flatten().is_some();
        if (st.tab < 4 && made) || (st.tab == 4 && bento_selected(app)) {
            run(app, &mut st, true);
        } else {
            st.dirty = false;
        }
    }
    let hint = match st.tab {
        0 => "A grid over the artboard of the selection (or the first artboard).",
        1..=3 => "Select the logo: one object for the mark, one for the logotype.",
        _ => "Select a rectangle to divide, or a bento grid to change.",
    };
    if let Some(m) = &st.message {
        ui.colored_label(ui.visuals().error_fg_color, m);
    } else {
        widgets::dim_label(ui, tl!(hint));
    }
    if st != before {
        set_pstate(ui.ctx(), KEY, st);
    }
}

pub fn menu(_app: &mut OracleVectorApp, ui: &mut Ui) {
    if widgets::menu_item(ui, tl!("Reset"), true, false) {
        set_pstate(ui.ctx(), KEY, State::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut OracleVectorApp, ctx: &egui::Context) {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(app, ui);
            menu(app, ui);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn every_tab_draws_headless_and_create_makes_its_grid() {
        let mut app = OracleVectorApp::new(oracle_vector_engine::Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let a = app.session.execute("shape.ellipse", &json!({"x": 20, "y": 20, "width": 80, "height": 80})).unwrap()["id"].clone();
        let b = app.session.execute("shape.rectangle", &json!({"x": 120, "y": 40, "width": 160, "height": 40})).unwrap()["id"].clone();
        app.session.execute("select.set", &json!({"ids": [a, b]})).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for tab in 0..TABS.len() {
            set_pstate(&ctx, KEY, State { tab, ..State::default() });
            frame(&mut app, &ctx);
            frame(&mut app, &ctx);
        }
        // What Create runs, tab by tab, on the logo.
        for tab in 0..4 {
            let mut st = State { tab, ..State::default() };
            run(&mut app, &mut st, false);
            assert!(st.message.is_none(), "{tab}: {:?}", st.message);
            assert!(st.layers[tab].is_some());
            // A live update replaces the layer instead of adding one.
            let layers = app.session.doc().unwrap().doc.layers.len();
            st.dirty = true;
            run(&mut app, &mut st, true);
            assert_eq!(app.session.doc().unwrap().doc.layers.len(), layers, "{tab}");
        }
    }
}
