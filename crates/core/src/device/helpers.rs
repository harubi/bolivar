//! Internal helpers shared by aggregator/collector/edge_probe devices.
//!
//! Extracted from the former `converter/base.rs` god-file in Task C2.

use crate::arena::types::{ArenaChar, ArenaItem};
use crate::interp::types::{PDFTextSeq, PDFTextSeqItem, PathSegment};
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::utils::{Matrix, apply_matrix_rect, mult_matrix};

use super::layout_analyzer::{PDFLayoutAnalyzer, PathOp};

/// Approximate equality for floating point.
pub(super) fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

pub(super) fn path_segments_to_path_ops(path: &[PathSegment]) -> Vec<PathOp> {
    path.iter()
        .map(|seg| match seg {
            PathSegment::MoveTo(x, y) => ('m', vec![*x, *y]),
            PathSegment::LineTo(x, y) => ('l', vec![*x, *y]),
            PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) => {
                ('c', vec![*x1, *y1, *x2, *y2, *x3, *y3])
            }
            PathSegment::ClosePath => ('h', vec![]),
        })
        .collect()
}

pub(super) struct FallbackCharRender<'a> {
    pub(super) char_matrix: Matrix,
    pub(super) fontsize: f64,
    pub(super) scaling: f64,
    pub(super) rise: f64,
    pub(super) cid: u32,
    pub(super) fallback_fontname: Option<&'a str>,
}

pub(super) fn render_char_without_font(
    analyzer: &mut PDFLayoutAnalyzer<'_>,
    graphicstate: &PDFGraphicState,
    render: FallbackCharRender<'_>,
) -> f64 {
    let FallbackCharRender {
        char_matrix,
        fontsize,
        scaling,
        rise,
        cid,
        fallback_fontname,
    } = render;
    let text = if (0x20..0x7f).contains(&cid) {
        char::from_u32(cid)
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("(cid:{})", cid))
    } else {
        format!("(cid:{})", cid)
    };

    let char_width = fontsize * scaling * 0.6;
    let descent = -fontsize * 0.25;
    let local_bbox = (0.0, descent + rise, char_width, descent + rise + fontsize);
    let bbox = apply_matrix_rect(char_matrix, local_bbox);

    let (a, b, c, d, _, _) = char_matrix;
    let upright = (a * d * scaling > 0.0) && (b * c <= 0.0);

    let mcid = analyzer.current_mcid();
    let tag = analyzer.current_tag_key();
    let ncolor_components = graphicstate.ncolor.components();
    let scolor_components = graphicstate.scolor.components();
    let ncolor = analyzer.arena.intern_color(ncolor_components.as_slice());
    let scolor = analyzer.arena.intern_color(scolor_components.as_slice());
    let fontname = fallback_fontname.unwrap_or("unknown");
    let text_key = analyzer.arena.intern(&text);
    let fontname_key = analyzer.arena.intern(fontname);
    let ncs_name = Some(analyzer.arena.intern(&graphicstate.ncs.name));
    let scs_name = Some(analyzer.arena.intern(&graphicstate.scs.name));
    let item = ArenaChar {
        bbox,
        text: text_key,
        fontname: fontname_key,
        size: bbox.3 - bbox.1,
        upright,
        adv: char_width,
        matrix: char_matrix,
        mcid,
        tag,
        ncs_name,
        scs_name,
        ncolor,
        scolor,
    };

    if let Some(ref mut container) = analyzer.cur_item {
        container.add(ArenaItem::Char(item));
    }

    char_width
}

pub(super) fn render_text_sequence(
    analyzer: &mut PDFLayoutAnalyzer<'_>,
    textstate: &mut PDFTextState,
    seq: &PDFTextSeq,
    graphicstate: &PDFGraphicState,
) {
    if textstate.render == 3 || textstate.render == 7 {
        return;
    }

    let ctm = analyzer.ctm;
    let matrix = mult_matrix(textstate.matrix, ctm);
    let fontsize = textstate.fontsize;
    let scaling = textstate.scaling * 0.01;
    let charspace = textstate.charspace * scaling;
    let wordspace = textstate.wordspace * scaling;
    let rise = textstate.rise;
    let dxscale = 0.001 * fontsize * scaling;

    let (mut x, mut y) = textstate.linematrix;
    let mut needcharspace = false;

    let font = textstate.font.clone();
    let fallback_fontname = textstate.fontname.clone();
    let is_vertical = font.as_ref().map(|f| f.is_vertical()).unwrap_or(false);

    for item in seq {
        match item {
            PDFTextSeqItem::Number(n) => {
                if is_vertical {
                    y -= n * dxscale;
                } else {
                    x -= n * dxscale;
                }
                needcharspace = true;
            }
            PDFTextSeqItem::Bytes(data) => {
                let cids: Vec<u32> = if let Some(ref font) = font {
                    font.decode(data)
                } else {
                    data.iter().map(|&b| b as u32).collect()
                };

                for cid in cids {
                    if needcharspace {
                        if is_vertical {
                            y += charspace;
                        } else {
                            x += charspace;
                        }
                    }

                    let char_matrix = (
                        matrix.0,
                        matrix.1,
                        matrix.2,
                        matrix.3,
                        matrix.0.mul_add(x, matrix.2 * y) + matrix.4,
                        matrix.1.mul_add(x, matrix.3 * y) + matrix.5,
                    );

                    let adv = if let Some(ref font) = font {
                        analyzer.render_char(
                            char_matrix,
                            font.as_ref(),
                            fontsize,
                            scaling,
                            rise,
                            cid,
                            &graphicstate.ncs,
                            graphicstate,
                        )
                    } else {
                        render_char_without_font(
                            analyzer,
                            graphicstate,
                            FallbackCharRender {
                                char_matrix,
                                fontsize,
                                scaling,
                                rise,
                                cid,
                                fallback_fontname: fallback_fontname.as_deref(),
                            },
                        )
                    };

                    if is_vertical {
                        y += adv;
                    } else {
                        x += adv;
                    }

                    if cid == 32 && wordspace != 0.0 {
                        if is_vertical {
                            y += wordspace;
                        } else {
                            x += wordspace;
                        }
                    }
                    needcharspace = true;
                }
            }
        }
    }

    textstate.linematrix = (x, y);
}
