use bumpalo::Bump;
use lasso::{Rodeo, Spur};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use super::types::{ArenaChar, ColorId};

type InternCache = FxHashMap<SmolStr, Spur>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ColorKey {
    inline: [u64; 4],
    extra: Box<[u64]>,
    len: usize,
}

impl ColorKey {
    fn from_slice(color: &[f64]) -> Self {
        let mut inline = [0; 4];
        for (slot, component) in inline.iter_mut().zip(color) {
            *slot = component.to_bits();
        }
        let extra = if color.len() > inline.len() {
            color[inline.len()..]
                .iter()
                .map(|component| component.to_bits())
                .collect()
        } else {
            Box::default()
        };
        Self {
            inline,
            extra,
            len: color.len(),
        }
    }
}

/// Page-scoped arena for allocation-free intermediates.
pub struct PageArena {
    bump: Bump,
    interner: Rodeo,
    intern_cache: InternCache,
    colors: Vec<Box<[f64]>>,
    color_index: FxHashMap<ColorKey, ColorId>,
}

pub struct ArenaContext<'a> {
    bump: &'a Bump,
    interner: &'a mut Rodeo,
    intern_cache: &'a mut InternCache,
    colors: &'a mut Vec<Box<[f64]>>,
    color_index: &'a mut FxHashMap<ColorKey, ColorId>,
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
            intern_cache: InternCache::default(),
            colors: Vec::new(),
            color_index: FxHashMap::default(),
        }
    }

    pub fn intern(&mut self, s: &str) -> Spur {
        intern_string(&mut self.interner, &mut self.intern_cache, s)
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
            intern_cache: &mut self.intern_cache,
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
        self.interner.clear();
        self.intern_cache.clear();
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
        intern_string(self.interner, self.intern_cache, s)
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

fn intern_string(interner: &mut Rodeo, cache: &mut InternCache, value: &str) -> Spur {
    cache.get(value).copied().unwrap_or_else(|| {
        let key = interner.get_or_intern(value);
        cache.insert(SmolStr::new(value), key);
        key
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_interning_handles_inline_and_long_values() {
        let mut arena = PageArena::new();
        let inline = arena.intern_color(&[1.0, 2.0, 3.0, 4.0]);
        let long = arena.intern_color(&[1.0, 2.0, 3.0, 4.0, 5.0]);

        assert_ne!(inline, long);
        assert_eq!(long, arena.intern_color(&[1.0, 2.0, 3.0, 4.0, 5.0]));
        assert_eq!(arena.color(long), &[1.0, 2.0, 3.0, 4.0, 5.0]);
    }
}
