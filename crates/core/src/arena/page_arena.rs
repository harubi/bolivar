use std::collections::HashMap;

use bumpalo::Bump;
use lasso::{Rodeo, Spur};

use super::types::{ArenaChar, ColorId};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ColorKey(Box<[u64]>);

impl ColorKey {
    fn from_slice(color: &[f64]) -> Self {
        let bits: Vec<u64> = color.iter().map(|c| c.to_bits()).collect();
        Self(bits.into_boxed_slice())
    }
}

/// Page-scoped arena for allocation-free intermediates.
pub struct PageArena {
    bump: Bump,
    interner: Rodeo,
    colors: Vec<Box<[f64]>>,
    color_index: HashMap<ColorKey, ColorId>,
}

pub struct ArenaContext<'a> {
    bump: &'a Bump,
    interner: &'a mut Rodeo,
    colors: &'a mut Vec<Box<[f64]>>,
    color_index: &'a mut HashMap<ColorKey, ColorId>,
}

pub trait ArenaLookup {
    fn resolve(&self, key: Spur) -> &str;
    fn color(&self, id: ColorId) -> &[f64];
}

pub trait ArenaBump {
    fn bump(&self) -> &Bump;
}

impl PageArena {
    pub fn new() -> Self {
        Self {
            bump: Bump::new(),
            interner: Rodeo::default(),
            colors: Vec::new(),
            color_index: HashMap::new(),
        }
    }

    pub fn intern(&mut self, s: &str) -> Spur {
        self.interner.get_or_intern(s)
    }

    pub fn resolve(&self, key: Spur) -> &str {
        self.interner.resolve(&key)
    }

    pub fn interner(&self) -> &Rodeo {
        &self.interner
    }

    pub const fn bump(&self) -> &Bump {
        &self.bump
    }

    pub fn context(&mut self) -> ArenaContext<'_> {
        ArenaContext {
            bump: &self.bump,
            interner: &mut self.interner,
            colors: &mut self.colors,
            color_index: &mut self.color_index,
        }
    }

    pub fn intern_color(&mut self, color: &[f64]) -> ColorId {
        let key = ColorKey::from_slice(color);
        if let Some(existing) = self.color_index.get(&key) {
            return *existing;
        }
        let id = ColorId::new(self.colors.len());
        self.colors.push(color.to_vec().into_boxed_slice());
        self.color_index.insert(key, id);
        id
    }

    pub fn color(&self, id: ColorId) -> &[f64] {
        &self.colors[id.index()]
    }

    pub fn alloc_char(&self, ch: ArenaChar) -> ArenaChar {
        ch
    }

    pub fn reset(&mut self) {
        self.bump.reset();
        self.interner = Rodeo::default();
        self.colors.clear();
        self.color_index.clear();
    }
}

impl Default for PageArena {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ArenaContext<'a> {
    pub const fn bump(&self) -> &'a Bump {
        self.bump
    }

    pub fn intern(&mut self, s: &str) -> Spur {
        self.interner.get_or_intern(s)
    }

    pub fn resolve(&self, key: Spur) -> &str {
        self.interner.resolve(&key)
    }

    pub fn intern_color(&mut self, color: &[f64]) -> ColorId {
        let key = ColorKey::from_slice(color);
        if let Some(existing) = self.color_index.get(&key) {
            return *existing;
        }
        let id = ColorId::new(self.colors.len());
        self.colors.push(color.to_vec().into_boxed_slice());
        self.color_index.insert(key, id);
        id
    }

    pub fn color(&self, id: ColorId) -> &[f64] {
        &self.colors[id.index()]
    }
}

impl ArenaLookup for PageArena {
    fn resolve(&self, key: Spur) -> &str {
        self.resolve(key)
    }

    fn color(&self, id: ColorId) -> &[f64] {
        self.color(id)
    }
}

impl<'a> ArenaLookup for ArenaContext<'a> {
    fn resolve(&self, key: Spur) -> &str {
        self.resolve(key)
    }

    fn color(&self, id: ColorId) -> &[f64] {
        self.color(id)
    }
}

impl ArenaBump for PageArena {
    fn bump(&self) -> &Bump {
        self.bump()
    }
}

impl<'a> ArenaBump for ArenaContext<'a> {
    fn bump(&self) -> &Bump {
        self.bump()
    }
}
