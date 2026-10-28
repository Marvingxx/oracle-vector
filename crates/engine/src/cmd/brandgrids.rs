//! Brand grids (`oracle-brandtools`): construction grids, clearspace and base grids drawn over a
//! logo or an artboard, and bento layouts that divide a rectangle. Everything is computed from the
//! selected art's own outlines and lands as ordinary paths and type: construction, clearspace and
//! base grids on a layer of their own above the current one, one group per element; a bento grid in
//! place of the rectangle it divides.
use std::sync::Arc;

use oracle_brandtools as bt;
use oracle_vector_color::{Color, Paint};
use oracle_vector_doc::{Appearance, CharStyle, Dash, Document, Justify, Node, NodeId, NodeKind, TextObject};
use oracle_vector_geom::{Affine, BezPath, PathEl, Point, Rect, shapes};
use serde_json::{Value, json};

use super::edit::selected_roots;
use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "brand.constructionGrid",
            "Construction Grid",
            ["Object", "Brand Grids"],
            None,
            "{params?: {horizontals, verticals, diagonals, circles, anchors, handles, outlines, spacing, measurements: bool, angleToleranceDeg, sensitivity (fraction of the logo size), circleFilter: all|big|small, circleThreshold, spacingPlacement: inside|outside, extent: logo|artboard, overshoot, anchorSize, handleSize, clean: bool, styles}, replace?: id of a layer an earlier run of a brand grid command made, removed in the same step (live updates)} the selected logo's construction (each selected object is a component: group the mark and the type separately) on a new layer above the current one, one group per element; the logo stays selected → {layer, groups: {name: count}}",
            has_selection,
            construction
        ),
        cmd!(
            "brand.clearspace",
            "Clearspace",
            ["Object", "Brand Grids"],
            None,
            "{params?: {unit: logomark|px|cm|in, size, invert: bool, fill: bool, minPrintMm, minScreenPx, color}, replace?: id of a layer an earlier run of a brand grid command made, removed in the same step (live updates)} the zone kept free around the selected logo, measured on its mark (the first selected object; `invert`: the second) or in a fixed unit → {layer, unit, zone: [x0,y0,x1,y1]}",
            has_selection,
            clearspace
        ),
        cmd!(
            "brand.baseGrid",
            "Base Grid",
            ["Object", "Brand Grids"],
            None,
            "{params?: {kind: square|isometric|hexagon|goldenRatio, size, stroke, color, goldenHeight, goldenSteps}, artboard?: index (default the one under the selection, else the first), makeGuides?: bool (square grid: ruler guides instead of paths), replace?: id of a layer an earlier run of a brand grid command made, removed in the same step (live updates)} a grid over the artboard on a new layer → {layer, count}",
            has_doc,
            base_grid
        ),
        cmd!(
            "brand.bento",
            "Bento Grid",
            ["Object", "Brand Grids"],
            None,
            "{params?: {style: simple|centered|brick|random, sections (1-64), radius, gutter, seed, fill, numbers: bool}} divides the selected rectangle (or the selected bento grid again, to shuffle it with a new `seed`) into tiles, in its place → {id, tiles}",
            has_selection,
            bento
        ),
        cmd!(
            "brand.lockup",
            "Lockup",
            ["Object", "Brand Grids"],
            None,
            "{params?: {kind: horizontal|vertical|condensed|partner, spacing (a multiple of the mark's smaller side), invert: bool (the second selected object is the mark), color}, partner?: id of the partner's logo (partner lockups; a placeholder frame without it), replace?: id of a layer an earlier run of a brand grid command made, removed in the same step (live updates)} copies of the selected mark and logotype arranged with one spacing rule, below the logo on a new layer, with the gaps shown → {layer, unit, gap}",
            has_selection,
            lockup
        ),
    ]
}

/// The layers the brand grid commands make: the only ones `replace` may remove.
const MADE: [&str; 5] = ["Grille de construction", "Zone de protection", "Grille de base", "Repères de grille", "Déclinaisons"];

fn replaced(cmd: &str, s: &Session, p: &Value) -> Result<Option<NodeId>> {
    let Some(v) = p.get("replace") else { return Ok(None) };
    if v.is_null() {
        return Ok(None);
    }
    let id = v.as_u64().map(NodeId).ok_or_else(|| bad(cmd, "replace: a layer id"))?;
    let doc = &s.doc()?.doc;
    // A layer since deleted or undone is simply not there any more.
    Ok(doc.node(id).filter(|n| n.is_layer() && n.name.as_deref().is_some_and(|name| MADE.contains(&name))).map(|_| id))
}

/// Where a new layer goes: in place of the replaced one, else above the current layer.
fn new_layer_at(doc: &mut Document, replace: Option<NodeId>, cur: Option<NodeId>) -> Result<(Option<NodeId>, usize)> {
    let old = replace.and_then(|id| doc.position(id));
    if let Some(id) = replace {
        doc.remove(id)?;
    }
    Ok(match old.or_else(|| cur.and_then(|c| doc.position(c)).map(|(p, i, d)| (p, i + 1, d))) {
        Some((parent, i, _)) => (parent, i),
        None => (None, usize::MAX),
    })
}

fn params<T: serde::de::DeserializeOwned + Default>(cmd: &str, p: &Value) -> Result<T> {
    match p.get("params") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(cmd, e.to_string())),
        None => Ok(T::default()),
    }
}

/// Text as its glyph outlines, so a live wordmark is measured like outlined type.
fn outline_text(n: &Node) -> Option<Node> {
    let NodeKind::Text(t) = &n.kind else { return None };
    let mut bp = oracle_vector_text::layout(oracle_vector_text::FontDb::global(), t).to_bezpath();
    bp.apply_affine(t.xf);
    Some(Node::path(n.id, oracle_vector_geom::PathData::from_bezpath(&bp), Appearance::default()))
}

fn bpt(p: Point) -> bt::Pt {
    bt::geom::pt(p.x, p.y)
}

fn contours(bp: &BezPath) -> Vec<bt::geom::Contour> {
    use bt::geom::{Contour, Seg};
    let mut out = Vec::new();
    let mut cur = Contour::default();
    let (mut start, mut last) = (bt::Pt::default(), bt::Pt::default());
    for el in bp.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if !cur.segs.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                start = bpt(p);
                last = start;
            }
            PathEl::LineTo(p) => {
                let p = bpt(p);
                if p.dist(last) > 1e-9 {
                    cur.segs.push(Seg::Line(last, p));
                }
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                let (c, p) = (bpt(c), bpt(p));
                cur.segs.push(Seg::Cubic(last, last + (c - last) * (2.0 / 3.0), p + (c - p) * (2.0 / 3.0), p));
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                let p = bpt(p);
                cur.segs.push(Seg::Cubic(last, bpt(a), bpt(b), p));
                last = p;
            }
            PathEl::ClosePath => {
                if last.dist(start) > 1e-9 {
                    cur.segs.push(Seg::Line(last, start));
                }
                cur.closed = true;
                last = start;
                if !cur.segs.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if !cur.segs.is_empty() {
        out.push(cur);
    }
    out
}

/// The selected objects as a logo, in document space: each selected object is a component.
fn logo_of(s: &Session, cmd: &str) -> Result<(bt::Logo, Vec<NodeId>)> {
    let roots = selected_roots(s)?;
    let doc = &s.doc()?.doc;
    let mut components = Vec::new();
    for id in &roots {
        let Some(n) = doc.node(*id) else { continue };
        let shapes: Vec<bt::logo::Shape> = n
            .clip_shapes(Some(&outline_text))
            .iter()
            .map(|(bp, _)| bt::logo::Shape { contours: contours(bp), fill: None })
            .filter(|sh| !sh.contours.is_empty())
            .collect();
        if !shapes.is_empty() {
            components.push(bt::logo::Component { name: n.name.clone().unwrap_or_default(), shapes });
        }
    }
    if components.is_empty() {
        return Err(bad(cmd, "select the logo's paths, compound paths, groups or type"));
    }
    // Cap the work: a construction grid is for a logo, not a whole illustration.
    let segments: usize = components.iter().flat_map(|c| c.contours()).map(|c| c.segs.len()).sum();
    if segments > 50_000 {
        return Err(bad(cmd, "the selection has too many path segments for a logo grid (over 50,000)"));
    }
    Ok((bt::Logo { components, width: 0.0, height: 0.0 }, roots))
}

fn color(hex: &str) -> Paint {
    Color::from_hex(hex).map_or(Paint::None, Paint::solid)
}

fn appearance(st: &bt::Style) -> Appearance {
    let fill = st.fill.as_deref().map_or(Paint::None, color);
    let stroke = st.stroke.as_deref().map_or(Paint::None, color);
    let mut a = Appearance::basic(fill, stroke, if st.stroke.is_some() { st.width } else { 0.0 });
    if let (Some(d), Some(stroke)) = (&st.dash, a.stroke_mut()) {
        stroke.dash = Some(Dash { pattern: d.clone(), ..Default::default() });
    }
    a
}

fn line(a: bt::Pt, b: bt::Pt) -> BezPath {
    let mut p = BezPath::new();
    p.move_to((a.x, a.y));
    p.line_to((b.x, b.y));
    p
}

/// An arc as cubic pieces of at most 90°.
fn arc(c: bt::Pt, r: f64, a: bt::Pt, b: bt::Pt, clockwise: bool) -> BezPath {
    let start = (a.y - c.y).atan2(a.x - c.x);
    let end = (b.y - c.y).atan2(b.x - c.x);
    let tau = std::f64::consts::TAU;
    let mut sweep = (end - start).rem_euclid(tau);
    if !clockwise {
        sweep -= tau;
    }
    let n = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().clamp(1.0, 8.0);
    let step = sweep / n;
    let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
    let at = |t: f64| (c.x + r * t.cos(), c.y + r * t.sin());
    let tangent = |t: f64| (-t.sin(), t.cos());
    let mut p = BezPath::new();
    p.move_to(at(start));
    for i in 0..n as usize {
        let (t0, t1) = (start + step * i as f64, start + step * (i + 1) as f64);
        let ((x0, y0), (x1, y1)) = (at(t0), at(t1));
        let ((dx0, dy0), (dx1, dy1)) = (tangent(t0), tangent(t1));
        p.curve_to((x0 + dx0 * k, y0 + dy0 * k), (x1 - dx1 * k, y1 - dy1 * k), (x1, y1));
    }
    p
}

/// One drawing item as a node; `None` for data that cannot be drawn (bad path data).
fn item_node(doc: &mut Document, item: &bt::Item, style: &bt::Style) -> Option<Node> {
    let a = appearance(style);
    let path = |doc: &mut Document, bp: BezPath, a: Appearance| Node::path(doc.alloc_id(), oracle_vector_geom::PathData::from_bezpath(&bp), a);
    let rect = |r: &bt::Rect| Rect::new(r.x0, r.y0, r.x1, r.y1);
    Some(match item {
        bt::Item::Line { a: p, b: q } => path(doc, line(*p, *q), a),
        bt::Item::Polygon { points } => {
            let (first, rest) = points.split_first()?;
            let mut bp = BezPath::new();
            bp.move_to((first.x, first.y));
            for q in rest {
                bp.line_to((q.x, q.y));
            }
            bp.close_path();
            path(doc, bp, a)
        }
        bt::Item::Circle { c, r } => {
            let pd = shapes::ellipse(Rect::new(c.x - r, c.y - r, c.x + r, c.y + r));
            Node::path(doc.alloc_id(), pd, a)
        }
        bt::Item::Arc { c, r, a: from, b: to, clockwise } => {
            let mut a = a;
            if let Some(f) = a.fill_mut() {
                f.paint = Paint::None;
            }
            path(doc, arc(*c, *r, *from, *to, *clockwise), a)
        }
        bt::Item::Rect { rect: r, radius } => {
            let pd = if *radius > 0.0 { shapes::rounded_rectangle(rect(r), *radius) } else { shapes::rectangle(rect(r)) };
            Node::path(doc.alloc_id(), pd, a)
        }
        bt::Item::Path { d, fill, transform } => {
            let mut bp = BezPath::from_svg(d).ok()?;
            if let Some(m) = transform {
                bp.apply_affine(Affine::new(*m));
            }
            let a = match fill {
                Some(f) => Appearance::basic(color(f), Paint::None, 0.0),
                None => a,
            };
            path(doc, bp, a)
        }
        bt::Item::Text { at, text, size, anchor } => {
            let ink = style.fill.as_deref().or(style.stroke.as_deref()).map_or(Paint::solid(Color::BLACK), color);
            let cs = CharStyle { size: *size, fill: ink.clone(), ..Default::default() };
            let mut t = TextObject::point(Point::new(at.x, at.y), text, cs);
            t.para.justify = match anchor {
                bt::drawing::Anchor::Start => Justify::Left,
                bt::drawing::Anchor::Middle => Justify::Center,
                bt::drawing::Anchor::End => Justify::Right,
            };
            t.system_layout = doc.setup.system_layout;
            super::typecmd::refresh_bounds(&mut t);
            let mut n = Node::new(doc.alloc_id(), NodeKind::Text(Box::new(t)));
            n.appearance = Appearance::basic(ink, Paint::None, 0.0).into();
            n
        }
    })
}

/// One group per drawing layer, named after it.
fn groups(doc: &mut Document, d: &bt::Drawing) -> Vec<(String, Node, usize)> {
    let mut out = Vec::new();
    for layer in &d.layers {
        let children: Vec<Arc<Node>> = layer.items.iter().filter_map(|i| item_node(doc, i, &layer.style)).map(Arc::new).collect();
        if children.is_empty() {
            continue;
        }
        let count = children.len();
        let mut g = Node::new(doc.alloc_id(), NodeKind::Group { children, clip: false });
        g.name = Some(layer.name.clone());
        g.opacity = layer.style.opacity.clamp(0.0, 1.0) as f32;
        out.push((layer.name.clone(), g, count));
    }
    out
}

/// Puts the drawing on a new layer above the current one (in place of `replace`). The selection is
/// left alone, so the logo stays selected for the next run.
fn on_new_layer(s: &mut Session, label: &str, name: &str, d: &bt::Drawing, replace: Option<NodeId>) -> Result<(NodeId, Value)> {
    let cur = s.doc()?.current_layer();
    let name = name.to_string();
    s.edit(label, |doc, _| {
        let (parent, index) = new_layer_at(doc, replace, cur)?;
        let layer = super::layer::make_layer(doc, Some(&name));
        let layer = doc.insert(parent, index, layer)?;
        let mut counts = serde_json::Map::new();
        for (name, g, count) in groups(doc, d) {
            doc.insert(Some(layer), usize::MAX, g)?;
            counts.insert(name, json!(count));
        }
        Ok((layer, Value::Object(counts)))
    })
}

fn construction(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "brand.constructionGrid";
    let params: bt::ConstructionParams = params(C, p)?;
    let (logo, _) = logo_of(s, C)?;
    let board = s.doc()?.doc.artboard_at(point_of(logo.bbox().center())).and_then(|i| s.doc().ok()?.doc.artboards.get(i).map(|a| a.rect));
    let d = bt::construction(&logo, board.map(|r| bt::Rect::new(r.x0, r.y0, r.x1, r.y1)), &params);
    let replace = replaced(C, s, p)?;
    let (layer, counts) = on_new_layer(s, "Construction Grid", "Grille de construction", &d, replace)?;
    Ok(json!({ "layer": layer.0, "groups": counts }))
}

fn point_of(p: bt::Pt) -> Point {
    Point::new(p.x, p.y)
}

fn clearspace(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "brand.clearspace";
    let params: bt::ClearspaceParams = params(C, p)?;
    let (logo, _) = logo_of(s, C)?;
    let (d, cs) = bt::clearspace(&logo, &params);
    let replace = replaced(C, s, p)?;
    let (layer, _) = on_new_layer(s, "Clearspace", "Zone de protection", &d, replace)?;
    let z = cs.zone;
    Ok(json!({ "layer": layer.0, "unit": cs.unit, "zone": [z.x0, z.y0, z.x1, z.y1] }))
}

fn base_grid(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "brand.baseGrid";
    let params: bt::BaseParams = params(C, p)?;
    let st = s.doc()?;
    let doc = &st.doc;
    let index = match p.get("artboard").and_then(Value::as_u64) {
        Some(i) => usize::try_from(i).map_err(|_| bad(C, "no such artboard"))?,
        None => st
            .selection
            .in_paint_order(doc)
            .first()
            .and_then(|id| doc.node(*id)?.visual_bounds())
            .and_then(|b| doc.artboard_at(b.center()))
            .unwrap_or(0),
    };
    let r = doc.artboards.get(index).map(|a| a.rect).ok_or_else(|| bad(C, "no such artboard"))?;
    let d = bt::base_grid(bt::Rect::new(r.x0, r.y0, r.x1, r.y1), &params);
    let replace = replaced(C, s, p)?;
    if p.get("makeGuides").and_then(Value::as_bool).unwrap_or(false) && !d.guides.is_empty() {
        let guides = d.guides.clone();
        let count = guides.len();
        let layer = s.edit("Base Grid Guides", |doc, _| {
            let (parent, index) = new_layer_at(doc, replace, None)?;
            let layer = super::layer::make_layer(doc, Some("Repères de grille"));
            let layer = doc.insert(parent, index, layer)?;
            for g in &guides {
                let (a, b) = match *g {
                    bt::Guide::Vertical(x) => (bt::geom::pt(x, r.y0 - 1e4), bt::geom::pt(x, r.y1 + 1e4)),
                    bt::Guide::Horizontal(y) => (bt::geom::pt(r.x0 - 1e4, y), bt::geom::pt(r.x1 + 1e4, y)),
                };
                let mut n = Node::path(doc.alloc_id(), oracle_vector_geom::PathData::from_bezpath(&line(a, b)), Appearance::default());
                if let NodeKind::Path { guide, .. } = &mut n.kind {
                    *guide = true;
                }
                doc.insert(Some(layer), usize::MAX, n)?;
            }
            Ok(layer)
        })?;
        return Ok(json!({ "layer": layer.0, "count": count }));
    }
    let (layer, counts) = on_new_layer(s, "Base Grid", "Grille de base", &d, replace)?;
    let count: u64 = counts.as_object().map_or(0, |m| m.values().filter_map(Value::as_u64).sum());
    Ok(json!({ "layer": layer.0, "count": count }))
}

fn bento(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "brand.bento";
    let params: bt::BentoParams = params(C, p)?;
    let (logo, roots) = logo_of(s, C)?;
    let area = logo.bbox();
    if area.width() < 1e-3 || area.height() < 1e-3 {
        return Err(bad(C, "select a rectangle with an area"));
    }
    let d = bt::bento(area, &params);
    let first = *roots.first().ok_or_else(|| bad(C, "nothing selected"))?;
    s.edit("Bento Grid", |doc, sel| {
        let (parent, index, _) = doc.position(first).ok_or_else(|| bad(C, "the selection is not in the document"))?;
        let mut children = Vec::new();
        let mut tiles = 0;
        for (_, g, count) in groups(doc, &d) {
            if tiles == 0 {
                tiles = count;
            }
            children.push(Arc::new(g));
        }
        let mut group = Node::new(doc.alloc_id(), NodeKind::Group { children, clip: false });
        group.name = Some("Bento".into());
        // The bento takes the place of what it divides.
        for id in &roots {
            doc.remove(*id)?;
        }
        let id = doc.insert(parent, index, group)?;
        sel.set([id]);
        Ok(json!({ "id": id.0, "tiles": tiles }))
    })
}

/// One selected object as a single-component logo.
fn single(n: &Node) -> bt::Logo {
    let shapes: Vec<bt::logo::Shape> = n
        .clip_shapes(Some(&outline_text))
        .iter()
        .map(|(bp, _)| bt::logo::Shape { contours: contours(bp), fill: None })
        .filter(|sh| !sh.contours.is_empty())
        .collect();
    bt::Logo { components: vec![bt::logo::Component { name: String::new(), shapes }], width: 0.0, height: 0.0 }
}

fn lockup(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "brand.lockup";
    let params: bt::LockupParams = params(C, p)?;
    let partner_id = p.get("partner").and_then(Value::as_u64).map(NodeId);
    let (mut logo, mut own) = logo_of(s, C)?;
    if logo.components.len() != own.len() {
        return Err(bad(C, "select the logo's objects: each selected object is one component"));
    }
    // The partner's logo is not part of ours, even when it is selected too.
    if let Some(pid) = partner_id
        && let Some(i) = own.iter().position(|r| *r == pid)
    {
        own.remove(i);
        logo.components.remove(i);
    }
    if own.is_empty() {
        return Err(bad(C, "select the logo as well as its partner"));
    }
    let partner = match partner_id {
        Some(pid) => Some((pid, single(s.doc()?.doc.node(pid).ok_or_else(|| bad(C, "partner: no such object"))?))),
        None => None,
    };
    let r = bt::lockup(&logo, partner.as_ref().map(|(_, l)| l), &params).map_err(|e| bad(C, e))?;
    // Below the logo, half its height away.
    let b = logo.bbox();
    let shift = Affine::translate((b.x0, b.y1 + b.height() * 0.5 + r.gap));
    let replace = replaced(C, s, p)?;
    let mark = usize::from(params.invert).min(own.len().saturating_sub(1));
    let cur = s.doc()?.current_layer();
    let kind = match params.kind {
        bt::LockupKind::Horizontal => "horizontale",
        bt::LockupKind::Vertical => "verticale",
        bt::LockupKind::Condensed => "condensée",
        bt::LockupKind::Partner => "partenaire",
    };
    let layer = s.edit("Lockup", |doc, _| {
        let (parent, index) = new_layer_at(doc, replace, cur)?;
        let layer = super::layer::make_layer(doc, Some("Déclinaisons"));
        let layer = doc.insert(parent, index, layer)?;
        let mut children: Vec<Arc<Node>> = Vec::new();
        for pl in &r.placements {
            let ids: Vec<NodeId> = match pl.part.as_str() {
                "mark" => own.get(mark).copied().into_iter().collect(),
                "type" => own.iter().enumerate().filter(|(i, _)| *i != mark).map(|(_, id)| *id).collect(),
                "logo" => own.clone(),
                _ => partner.as_ref().map(|(id, _)| *id).into_iter().collect(),
            };
            for id in ids {
                let Some(n) = doc.node(id).map(|n| (*n).clone()) else { continue };
                let mut copy = doc.reid(&n);
                copy.transform(shift * Affine::new(pl.transform), true);
                children.push(Arc::new(copy));
            }
        }
        // The gap boxes, their guides, the divider and the partner's frame, moved alike.
        for (name, mut g, _) in groups(doc, &r.drawing) {
            if name == "Logo" {
                continue;
            }
            g.transform(shift, true);
            children.push(Arc::new(g));
        }
        let mut group = Node::new(doc.alloc_id(), NodeKind::Group { children, clip: false });
        group.name = Some(format!("Déclinaison {kind}"));
        doc.insert(Some(layer), usize::MAX, group)?;
        Ok(layer)
    })?;
    Ok(json!({ "layer": layer.0, "unit": r.unit, "gap": r.gap }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
        s
    }

    fn select(s: &mut Session, ids: &[u64]) {
        s.execute("select.set", &json!({ "ids": ids })).unwrap();
    }

    fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> u64 {
        s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap()["id"].as_u64().unwrap()
    }

    fn ellipse(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> u64 {
        s.execute("shape.ellipse", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap()["id"].as_u64().unwrap()
    }

    fn group_count(s: &Session, layer: u64, name: &str) -> usize {
        let doc = &s.doc().unwrap().doc;
        let l = doc.node(NodeId(layer)).unwrap();
        let g = l.children().unwrap().iter().find(|g| g.name.as_deref() == Some(name)).unwrap_or_else(|| panic!("no {name}"));
        g.children().map_or(0, |c| c.len())
    }

    #[test]
    fn a_construction_grid_lands_on_its_own_layer_with_one_group_per_element() {
        let mut s = session();
        let mark = ellipse(&mut s, 100.0, 100.0, 160.0, 160.0);
        let word = rect(&mut s, 300.0, 140.0, 200.0, 80.0);
        select(&mut s, &[mark, word]);
        let layers_before = s.doc().unwrap().doc.layers.len();
        let r = s.execute("brand.constructionGrid", &json!({})).unwrap();
        let layer = r["layer"].as_u64().unwrap();
        assert_eq!(s.doc().unwrap().doc.layers.len(), layers_before + 1);
        assert_eq!(s.doc().unwrap().doc.node(NodeId(layer)).unwrap().name.as_deref(), Some("Grille de construction"));
        // The circle of the ellipse, its four extremes and the rectangle's edges.
        assert_eq!(group_count(&s, layer, "Cercles"), 1);
        assert_eq!(r["groups"]["Horizontales"], 4, "{r}");
        assert_eq!(r["groups"]["Verticales"], 4, "{r}");
        // One undo step takes it all away.
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().doc.layers.len(), layers_before);
    }

    #[test]
    fn clearspace_is_measured_on_the_mark() {
        let mut s = session();
        let mark = rect(&mut s, 100.0, 100.0, 40.0, 60.0);
        let word = rect(&mut s, 160.0, 110.0, 200.0, 40.0);
        select(&mut s, &[mark, word]);
        let r = s.execute("brand.clearspace", &json!({})).unwrap();
        assert!((r["unit"].as_f64().unwrap() - 40.0).abs() < 1e-6, "{r}");
        assert!((r["zone"][0].as_f64().unwrap() - 60.0).abs() < 1e-6);
        let half = s.execute("brand.clearspace", &json!({"params": {"unit": "px", "size": 12}})).unwrap();
        assert_eq!(half["unit"], 12.0);
        assert!(s.execute("brand.clearspace", &json!({"params": {"unit": "furlong"}})).is_err());
    }

    #[test]
    fn a_base_grid_covers_the_artboard_or_becomes_guides() {
        let mut s = session();
        let r = s.execute("brand.baseGrid", &json!({"params": {"kind": "square", "size": 100}})).unwrap();
        // 800 × 600 centred: x = 0…800 (9 lines), y = 0…600 (7 lines).
        assert_eq!(r["count"], 16, "{r}");
        let g = s.execute("brand.baseGrid", &json!({"params": {"kind": "square", "size": 100}, "makeGuides": true})).unwrap();
        assert_eq!(g["count"], 16);
        assert!(s.execute("brand.baseGrid", &json!({"params": {"kind": "goldenRatio"}})).is_ok());
        assert!(s.execute("brand.baseGrid", &json!({"artboard": 9})).is_err());
    }

    #[test]
    fn a_bento_takes_the_place_of_its_rectangle_and_reshuffles() {
        let mut s = session();
        let r = rect(&mut s, 0.0, 0.0, 600.0, 400.0);
        select(&mut s, &[r]);
        let b = s.execute("brand.bento", &json!({"params": {"style": "random", "sections": 7, "gutter": 10}})).unwrap();
        assert_eq!(b["tiles"], 7);
        assert!(s.doc().unwrap().doc.node(NodeId(r)).is_none());
        // The bento's bounds are the rectangle's: shuffling it again keeps the area.
        let again = s.execute("brand.bento", &json!({"params": {"style": "random", "sections": 7, "seed": 4}})).unwrap();
        assert_eq!(again["tiles"], 7);
        let doc = &s.doc().unwrap().doc;
        let bounds = doc.node(NodeId(again["id"].as_u64().unwrap())).unwrap().visual_bounds().unwrap();
        assert!((bounds.width() - 600.0).abs() < 1.5 && (bounds.height() - 400.0).abs() < 1.5, "{bounds:?}");
    }

    #[test]
    fn replace_swaps_the_layer_a_run_made_and_never_another() {
        let mut s = session();
        let mark = rect(&mut s, 100.0, 100.0, 40.0, 60.0);
        let word = rect(&mut s, 160.0, 110.0, 200.0, 40.0);
        select(&mut s, &[mark, word]);
        let n = s.doc().unwrap().doc.layers.len();
        let first = s.execute("brand.clearspace", &json!({})).unwrap()["layer"].as_u64().unwrap();
        // The logo is still selected: the next run (a live update) replaces the first.
        let second = s.execute("brand.clearspace", &json!({"params": {"size": 0.5}, "replace": first})).unwrap();
        assert_eq!(s.doc().unwrap().doc.layers.len(), n + 1);
        assert!(s.doc().unwrap().doc.node(NodeId(first)).is_none());
        assert!((second["unit"].as_f64().unwrap() - 20.0).abs() < 1e-6);
        // A layer the user made is never removed.
        let own = s.execute("layer.new", &json!({"name": "Mes notes"})).unwrap()["id"].as_u64().unwrap();
        select(&mut s, &[mark, word]);
        s.execute("brand.clearspace", &json!({"replace": own})).unwrap();
        assert!(s.doc().unwrap().doc.node(NodeId(own)).is_some());
    }

    #[test]
    fn a_lockup_copies_the_mark_and_type_with_one_gap() {
        let mut s = session();
        let mark = rect(&mut s, 100.0, 100.0, 40.0, 40.0);
        let word = rect(&mut s, 160.0, 110.0, 200.0, 20.0);
        select(&mut s, &[mark, word]);
        let r = s.execute("brand.lockup", &json!({"params": {"kind": "vertical", "spacing": 0.5}})).unwrap();
        assert!((r["gap"].as_f64().unwrap() - 20.0).abs() < 1e-6, "{r}");
        // The originals are untouched and still selected.
        assert_eq!(selected_roots(&s).unwrap(), vec![NodeId(mark), NodeId(word)]);
        {
            let doc = &s.doc().unwrap().doc;
            let layer = doc.node(NodeId(r["layer"].as_u64().unwrap())).unwrap();
            let group = layer.children().unwrap().first().unwrap();
            assert_eq!(group.name.as_deref(), Some("Déclinaison verticale"));
            // Mark and type copies come first: the type 20 below the mark, both centred.
            let kids = group.children().unwrap();
            let (m, t) = (kids[0].visual_bounds().unwrap(), kids[1].visual_bounds().unwrap());
            assert!((t.y0 - m.y1 - 20.0).abs() < 1.5, "{m:?} {t:?}");
            assert!((t.center().x - m.center().x).abs() < 1e-6);
        }
        let partner = rect(&mut s, 500.0, 100.0, 80.0, 40.0);
        select(&mut s, &[mark, word]);
        assert!(s.execute("brand.lockup", &json!({"params": {"kind": "partner"}, "partner": partner})).is_ok());
        let one = rect(&mut s, 0.0, 0.0, 5.0, 5.0);
        select(&mut s, &[one]);
        assert!(s.execute("brand.lockup", &json!({})).is_err());
    }

    #[test]
    fn nothing_drawable_is_refused() {
        let mut s = session();
        assert!(s.execute("brand.constructionGrid", &json!({})).is_err());
        let r = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
        select(&mut s, &[r]);
        assert!(s.execute("brand.constructionGrid", &json!({"params": {"sensitivity": "high"}})).is_err());
    }
}
