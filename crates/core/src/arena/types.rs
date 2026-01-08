use bumpalo::collections::Vec as BumpVec;
use lasso::Spur;

use crate::arena::{ArenaBump, ArenaLookup};
use crate::layout::types::{LTChar, LTCurve, LTFigure, LTImage, LTItem, LTLine, LTPage, LTRect};
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
    pub fn materialize(&self, arena: &impl ArenaLookup) -> LTChar {
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
pub enum ArenaItem<'a> {
    Char(ArenaChar),
    Line(ArenaLine),
    Rect(ArenaRect),
    Curve(ArenaCurve),
    Image(ArenaImage<'a>),
    Figure(ArenaFigure<'a>),
}

#[derive(Debug, Clone)]
pub struct ArenaPage<'a> {
    pub pageid: i32,
    pub bbox: Rect,
    pub rotate: f64,
    pub items: BumpVec<'a, ArenaItem<'a>>,
}

impl<'a> ArenaPage<'a> {
    pub fn new_in(arena: &'a impl ArenaBump, pageid: i32, bbox: Rect) -> Self {
        Self {
            pageid,
            bbox,
            rotate: 0.0,
            items: BumpVec::new_in(arena.bump()),
        }
    }

    pub fn add(&mut self, item: ArenaItem<'a>) {
        self.items.push(item);
    }

    pub fn materialize(self, arena: &impl ArenaLookup) -> LTPage {
        let mut page = LTPage::new(self.pageid, self.bbox, self.rotate);
        for item in self.items {
            page.add(materialize_item(item, arena));
        }
        page
    }
}

fn materialize_item(item: ArenaItem<'_>, arena: &impl ArenaLookup) -> LTItem {
    match item {
        ArenaItem::Char(ch) => LTItem::Char(ch.materialize(arena)),
        ArenaItem::Line(line) => LTItem::Line(line.materialize(arena)),
        ArenaItem::Rect(rect) => LTItem::Rect(rect.materialize(arena)),
        ArenaItem::Curve(curve) => LTItem::Curve(curve.materialize(arena)),
        ArenaItem::Image(image) => LTItem::Image(image.materialize(arena)),
        ArenaItem::Figure(figure) => LTItem::Figure(Box::new(figure.materialize(arena))),
    }
}

#[derive(Debug, Clone)]
pub struct ArenaImage<'a> {
    pub name: Spur,
    pub bbox: Rect,
    pub srcsize: (Option<i32>, Option<i32>),
    pub imagemask: bool,
    pub bits: i32,
    pub colorspace: BumpVec<'a, Spur>,
}

impl<'a> ArenaImage<'a> {
    pub fn materialize(&self, arena: &impl ArenaLookup) -> LTImage {
        let name = arena.resolve(self.name);
        let colorspace = self
            .colorspace
            .iter()
            .map(|cs| arena.resolve(*cs).to_string())
            .collect();
        LTImage::new(
            name,
            self.bbox,
            self.srcsize,
            self.imagemask,
            self.bits,
            colorspace,
        )
    }
}

#[derive(Debug, Clone)]
pub struct ArenaFigure<'a> {
    pub name: Spur,
    pub bbox: Rect,
    pub matrix: Matrix,
    pub items: BumpVec<'a, ArenaItem<'a>>,
}

impl<'a> ArenaFigure<'a> {
    pub fn new_in(arena: &'a impl ArenaBump, name: Spur, bbox: Rect, matrix: Matrix) -> Self {
        Self {
            name,
            bbox,
            matrix,
            items: BumpVec::new_in(arena.bump()),
        }
    }

    pub fn add(&mut self, item: ArenaItem<'a>) {
        self.items.push(item);
    }

    pub fn materialize(self, arena: &impl ArenaLookup) -> LTFigure {
        let name = arena.resolve(self.name).to_string();
        let mut fig = LTFigure::new(&name, self.bbox, self.matrix);
        for item in self.items {
            fig.add(materialize_item(item, arena));
        }
        fig
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
    pub fn materialize(&self, arena: &impl ArenaLookup) -> LTCurve {
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
    pub fn materialize(&self, arena: &impl ArenaLookup) -> LTLine {
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
    pub fn materialize(&self, arena: &impl ArenaLookup) -> LTRect {
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
