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

/// Where a repeated value comes from, for [`ArenaContext::intern_at`] and
/// [`ArenaContext::intern_color_at`].
#[derive(Clone, Copy)]
pub enum MemoSlot {
    FontName,
    NonStrokingSpace,
    StrokingSpace,
}

/// Which color an interning call site remembers.
#[derive(Clone, Copy)]
pub enum ColorSlot {
    NonStroking,
    Stroking,
}

/// The last value interned at each call site, with its id. Runs of characters share their
/// font, color spaces and colors, so most lookups end here, after one short comparison and no
/// hashing. Colors compare by bit pattern, as [`ColorKey`] does.
#[derive(Default)]
struct InternMemo {
    strings: [Option<(SmolStr, Spur)>; 3],
    colors: [Option<([u64; 4], usize, ColorId)>; 2],
}

/// Page-scoped arena for allocation-free intermediates.
pub struct PageArena {
    bump: Bump,
    interner: Rodeo,
    intern_cache: InternCache,
    colors: Vec<Box<[f64]>>,
    color_index: FxHashMap<ColorKey, ColorId>,
    memo: InternMemo,
}

pub struct ArenaContext<'a> {
    bump: &'a Bump,
    interner: &'a mut Rodeo,
    intern_cache: &'a mut InternCache,
    colors: &'a mut Vec<Box<[f64]>>,
    color_index: &'a mut FxHashMap<ColorKey, ColorId>,
    memo: &'a mut InternMemo,
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
            memo: InternMemo::default(),
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
            memo: &mut self.memo,
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
        // Remembered ids belong to the interner just cleared.
        self.memo = InternMemo::default();
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

    /// [`Self::intern`], remembering the last value of `slot`.
    pub fn intern_at(&mut self, slot: MemoSlot, s: &str) -> Spur {
        if let Some((last, key)) = &self.memo.strings[slot as usize]
            && last == s
        {
            return *key;
        }
        let key = intern_string(self.interner, self.intern_cache, s);
        self.memo.strings[slot as usize] = Some((SmolStr::new(s), key));
        key
    }

    /// [`Self::intern_color`], remembering the last color of `slot`.
    pub fn intern_color_at(&mut self, slot: ColorSlot, color: &[f64]) -> ColorId {
        let bits = color_bits(color);
        if let Some((last, len, id)) = self.memo.colors[slot as usize]
            && len == color.len()
            && Some(last) == bits
        {
            return id;
        }
        let id = self.intern_color(color);
        self.memo.colors[slot as usize] = bits.map(|bits| (bits, color.len(), id));
        id
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

/// A color of at most four components as bit patterns; longer colors are not remembered.
fn color_bits(color: &[f64]) -> Option<[u64; 4]> {
    let mut bits = [0; 4];
    if color.len() > bits.len() {
        return None;
    }
    for (slot, component) in bits.iter_mut().zip(color) {
        *slot = component.to_bits();
    }
    Some(bits)
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
