//! PDF content stream operator implementations.
//!
//! Operators are grouped by category:
//! - `graphics_state` - State stack and transforms (q, Q, cm, w, J, j, M, d, ri, i, gs)
//! - `color` - Color space and values (G, g, RG, rg, K, k, CS, cs, SC, SCN, sc, scn)
//! - `path` - Path construction and painting (m, l, c, v, y, h, re, S, s, f, F, f\*, B, B\*, b, b\*, n, W, W\*)
//! - `text` - Text state and rendering (BT, ET, Tc, Tw, Tz, TL, Tf, Tr, Ts, Td, TD, Tm, T\*, Tj, TJ, ', ")
//! - `xobject` - XObjects and marked content (Do, BI, ID, EI, BMC, BDC, EMC, MP, DP)

mod color;
mod graphics_state;
mod path;
mod text;
mod xobject;

// Note: graphics_state.rs defines an impl block for PDFPageInterpreter,
// so no pub use is needed - the methods are automatically available on the type.
