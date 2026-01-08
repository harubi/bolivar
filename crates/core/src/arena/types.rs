use lasso::Spur;

use crate::arena::PageArena;
use crate::layout::types::{LTChar, LTCurve, LTItem, LTLine, LTPage, LTRect};
use crate::utils::{Matrix, Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorId(usize);

impl ColorId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct ArenaChar {
    pub bbox: Rect,
    pub text: Spur,
    pub fontname: Spur,
    pub size: f64,
    pub upright: bool,
    pub adv: f64,
    pub matrix: Matrix,
    pub mcid: Option<i32>,
    pub tag: Option<Spur>,
    pub ncs_name: Option<Spur>,
    pub scs_name: Option<Spur>,
    pub ncolor: ColorId,
    pub scolor: ColorId,
}

impl ArenaChar {
    pub fn materialize(&self, arena: &PageArena) -> LTChar {
        let text = arena.resolve(self.text);
        let fontname = arena.resolve(self.fontname);
        let tag = self.tag.map(|t| arena.resolve(t).to_string());
        let ncolor = Some(arena.color(self.ncolor).to_vec());
        let scolor = Some(arena.color(self.scolor).to_vec());

        let mut ltchar = LTChar::with_colors_matrix(
            self.bbox,
            text,
            fontname,
            self.size,
            self.upright,
            self.adv,
            self.matrix,
            self.mcid,
            tag,
            ncolor,
            scolor,
        );

        if let Some(ncs) = self.ncs_name {
            ltchar.set_ncs(Some(arena.resolve(ncs).to_string()));
        }
        if let Some(scs) = self.scs_name {
            ltchar.set_scs(Some(arena.resolve(scs).to_string()));
        }

        ltchar
    }
}

#[derive(Debug, Clone)]
pub enum ArenaItem {
    Char(ArenaChar),
    Line(ArenaLine),
    Rect(ArenaRect),
    Curve(ArenaCurve),
}

#[derive(Debug, Clone)]
pub struct ArenaPage {
    pub pageid: i32,
    pub bbox: Rect,
    pub rotate: f64,
    pub items: Vec<ArenaItem>,
}

impl ArenaPage {
    pub fn new(pageid: i32, bbox: Rect) -> Self {
        Self {
            pageid,
            bbox,
            rotate: 0.0,
            items: Vec::new(),
        }
    }

    pub fn add(&mut self, item: ArenaItem) {
        self.items.push(item);
    }

    pub fn materialize(self, arena: &PageArena) -> LTPage {
        let mut page = LTPage::new(self.pageid, self.bbox, self.rotate);
        for item in self.items {
            match item {
                ArenaItem::Char(ch) => page.add(LTItem::Char(ch.materialize(arena))),
                ArenaItem::Line(line) => page.add(LTItem::Line(line.materialize(arena))),
                ArenaItem::Rect(rect) => page.add(LTItem::Rect(rect.materialize(arena))),
                ArenaItem::Curve(curve) => page.add(LTItem::Curve(curve.materialize(arena))),
            }
        }
        page
    }
}

#[derive(Debug, Clone)]
pub struct ArenaCurve {
    pub linewidth: f64,
    pub pts: Vec<Point>,
    pub stroke: bool,
    pub fill: bool,
    pub evenodd: bool,
    pub stroking_color: ColorId,
    pub non_stroking_color: ColorId,
    pub original_path: Option<Vec<(char, Vec<Point>)>>,
    pub dashing_style: Option<(Vec<f64>, f64)>,
    pub mcid: Option<i32>,
    pub tag: Option<Spur>,
}

impl ArenaCurve {
    pub fn materialize(&self, arena: &PageArena) -> LTCurve {
        let stroking_color = Some(arena.color(self.stroking_color).to_vec());
        let non_stroking_color = Some(arena.color(self.non_stroking_color).to_vec());
        let mut curve = if self.original_path.is_some() || self.dashing_style.is_some() {
            LTCurve::new_with_dashing(
                self.linewidth,
                self.pts.clone(),
                self.stroke,
                self.fill,
                self.evenodd,
                stroking_color,
                non_stroking_color,
                self.original_path.clone(),
                self.dashing_style.clone(),
            )
        } else {
            LTCurve::new(
                self.linewidth,
                self.pts.clone(),
                self.stroke,
                self.fill,
                self.evenodd,
                stroking_color,
                non_stroking_color,
            )
        };
        let tag = self.tag.map(|t| arena.resolve(t).to_string());
        curve.set_marked_content(self.mcid, tag);
        curve
    }
}

#[derive(Debug, Clone)]
pub struct ArenaLine {
    pub linewidth: f64,
    pub p0: Point,
    pub p1: Point,
    pub stroke: bool,
    pub fill: bool,
    pub evenodd: bool,
    pub stroking_color: ColorId,
    pub non_stroking_color: ColorId,
    pub original_path: Option<Vec<(char, Vec<Point>)>>,
    pub dashing_style: Option<(Vec<f64>, f64)>,
    pub mcid: Option<i32>,
    pub tag: Option<Spur>,
}

impl ArenaLine {
    pub fn materialize(&self, arena: &PageArena) -> LTLine {
        let stroking_color = Some(arena.color(self.stroking_color).to_vec());
        let non_stroking_color = Some(arena.color(self.non_stroking_color).to_vec());
        let mut line = if self.original_path.is_some() || self.dashing_style.is_some() {
            LTLine::new_with_dashing(
                self.linewidth,
                self.p0,
                self.p1,
                self.stroke,
                self.fill,
                self.evenodd,
                stroking_color,
                non_stroking_color,
                self.original_path.clone(),
                self.dashing_style.clone(),
            )
        } else {
            LTLine::new(
                self.linewidth,
                self.p0,
                self.p1,
                self.stroke,
                self.fill,
                self.evenodd,
                stroking_color,
                non_stroking_color,
            )
        };
        let tag = self.tag.map(|t| arena.resolve(t).to_string());
        line.set_marked_content(self.mcid, tag);
        line
    }
}

#[derive(Debug, Clone)]
pub struct ArenaRect {
    pub linewidth: f64,
    pub bbox: Rect,
    pub stroke: bool,
    pub fill: bool,
    pub evenodd: bool,
    pub stroking_color: ColorId,
    pub non_stroking_color: ColorId,
    pub original_path: Option<Vec<(char, Vec<Point>)>>,
    pub dashing_style: Option<(Vec<f64>, f64)>,
    pub mcid: Option<i32>,
    pub tag: Option<Spur>,
}

impl ArenaRect {
    pub fn materialize(&self, arena: &PageArena) -> LTRect {
        let stroking_color = Some(arena.color(self.stroking_color).to_vec());
        let non_stroking_color = Some(arena.color(self.non_stroking_color).to_vec());
        let mut rect = if self.original_path.is_some() || self.dashing_style.is_some() {
            LTRect::new_with_dashing(
                self.linewidth,
                self.bbox,
                self.stroke,
                self.fill,
                self.evenodd,
                stroking_color,
                non_stroking_color,
                self.original_path.clone(),
                self.dashing_style.clone(),
            )
        } else {
            LTRect::new(
                self.linewidth,
                self.bbox,
                self.stroke,
                self.fill,
                self.evenodd,
                stroking_color,
                non_stroking_color,
            )
        };
        let tag = self.tag.map(|t| arena.resolve(t).to_string());
        rect.set_marked_content(self.mcid, tag);
        rect
    }
}
