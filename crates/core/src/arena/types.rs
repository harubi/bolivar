use std::sync::Arc;

use bumpalo::collections::Vec as BumpVec;
use lasso::Spur;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::arena::{ArenaBump, ArenaLookup};
use crate::layout::types::{
    CharStyle, LTChar, LTCurve, LTFigure, LTImage, LTItem, LTLine, LTPage, LTRect,
};
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

/// A character's shared state, by the arena's interned ids: font, mcid, tag, colour spaces and
/// colours.
type StyleKey = (
    Spur,
    Option<i32>,
    Option<Spur>,
    Option<Spur>,
    Option<Spur>,
    ColorId,
    ColorId,
);

/// One shared [`CharStyle`] per distinct style, for the characters of one page.
#[derive(Default)]
pub(crate) struct StyleCache(FxHashMap<StyleKey, Arc<CharStyle>>);

impl ArenaChar {
    /// One character on its own: its style is built for it alone.
    pub fn materialize(&self, arena: &impl ArenaLookup) -> LTChar {
        self.with_style(arena, Arc::new(self.style(arena)))
    }

    /// One of many: characters with the same style share one [`CharStyle`].
    fn materialize_with(&self, arena: &impl ArenaLookup, styles: &mut StyleCache) -> LTChar {
        let key = (
            self.fontname,
            self.mcid,
            self.tag,
            self.ncs_name,
            self.scs_name,
            self.ncolor,
            self.scolor,
        );
        let style = Arc::clone(
            styles
                .0
                .entry(key)
                .or_insert_with(|| Arc::new(self.style(arena))),
        );
        self.with_style(arena, style)
    }

    fn style(&self, arena: &impl ArenaLookup) -> CharStyle {
        CharStyle {
            fontname: SmolStr::new(arena.resolve(self.fontname)),
            mcid: self.mcid,
            tag: self.tag.map(|key| SmolStr::new(arena.resolve(key))),
            ncs: self.ncs_name.map(|key| SmolStr::new(arena.resolve(key))),
            scs: self.scs_name.map(|key| SmolStr::new(arena.resolve(key))),
            non_stroking_color: Some(arena.color(self.ncolor).to_vec()),
            stroking_color: Some(arena.color(self.scolor).to_vec()),
        }
    }

    fn with_style(&self, arena: &impl ArenaLookup, style: Arc<CharStyle>) -> LTChar {
        LTChar::from_arena_parts(
            self.bbox,
            arena.resolve(self.text),
            self.size,
            self.upright,
            self.adv,
            self.matrix,
            style,
        )
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
        let item_count = self.items.len();
        let mut page = LTPage::new(self.pageid, self.bbox, self.rotate);
        page.reserve_items(item_count);
        let mut styles = StyleCache::default();
        for item in self.items {
            page.add(materialize_item(item, arena, &mut styles));
        }
        page
    }
}

fn materialize_item(
    item: ArenaItem<'_>,
    arena: &impl ArenaLookup,
    styles: &mut StyleCache,
) -> LTItem {
    match item {
        ArenaItem::Char(ch) => LTItem::Char(ch.materialize_with(arena, styles)),
        ArenaItem::Line(line) => LTItem::Line(line.materialize(arena)),
        ArenaItem::Rect(rect) => LTItem::Rect(rect.materialize(arena)),
        ArenaItem::Curve(curve) => LTItem::Curve(curve.materialize(arena)),
        ArenaItem::Image(image) => LTItem::Image(image.materialize(arena)),
        ArenaItem::Figure(figure) => {
            LTItem::Figure(Box::new(figure.materialize_with(arena, styles)))
        }
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
        self.materialize_with(arena, &mut StyleCache::default())
    }

    fn materialize_with(self, arena: &impl ArenaLookup, styles: &mut StyleCache) -> LTFigure {
        let item_count = self.items.len();
        let name = arena.resolve(self.name).to_string();
        let mut fig = LTFigure::new(&name, self.bbox, self.matrix);
        fig.reserve_items(item_count);
        for item in self.items {
            fig.add(materialize_item(item, arena, styles));
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
