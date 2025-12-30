//! RFC 4013 SASLprep implementation for password normalization.
//!
//! This implements the stringprep profile defined in RFC 4013, which is used
//! for preparing passwords in SASL authentication mechanisms.

use unicode_normalization::UnicodeNormalization;

use crate::{PdfError, Result};

/// RFC 3454 Table B.1: Characters commonly mapped to nothing.
fn in_table_b1(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'          // SOFT HYPHEN
        | '\u{034F}'        // COMBINING GRAPHEME JOINER
        | '\u{1806}'        // MONGOLIAN TODO SOFT HYPHEN
        | '\u{180B}'        // MONGOLIAN FREE VARIATION SELECTOR ONE
        | '\u{180C}'        // MONGOLIAN FREE VARIATION SELECTOR TWO
        | '\u{180D}'        // MONGOLIAN FREE VARIATION SELECTOR THREE
        | '\u{200B}'        // ZERO WIDTH SPACE
        | '\u{200C}'        // ZERO WIDTH NON-JOINER
        | '\u{200D}'        // ZERO WIDTH JOINER
        | '\u{2060}'        // WORD JOINER
        | '\u{FE00}'
            ..='\u{FE0F}' // VARIATION SELECTOR-1 through -16
        | '\u{FEFF}' // ZERO WIDTH NO-BREAK SPACE (BOM)
    )
}

/// RFC 3454 Table C.1.2: Non-ASCII space characters.
fn in_table_c12(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}'          // NO-BREAK SPACE
        | '\u{1680}'        // OGHAM SPACE MARK
        | '\u{2000}'
            ..='\u{200A}' // EN QUAD through HAIR SPACE
        | '\u{2028}'        // LINE SEPARATOR
        | '\u{2029}'        // PARAGRAPH SEPARATOR
        | '\u{202F}'        // NARROW NO-BREAK SPACE
        | '\u{205F}'        // MEDIUM MATHEMATICAL SPACE
        | '\u{3000}' // IDEOGRAPHIC SPACE
    )
}

/// RFC 3454 Table C.2.1: ASCII control characters.
fn in_table_c21(c: char) -> bool {
    matches!(c, '\u{0000}'..='\u{001F}' | '\u{007F}')
}

/// RFC 3454 Table C.2.2: Non-ASCII control characters.
fn in_table_c22(c: char) -> bool {
    matches!(
        c,
        '\u{0080}'..='\u{009F}'
        | '\u{06DD}'        // ARABIC END OF AYAH
        | '\u{070F}'        // SYRIAC ABBREVIATION MARK
        | '\u{180E}'        // MONGOLIAN VOWEL SEPARATOR
        | '\u{200C}'        // ZERO WIDTH NON-JOINER
        | '\u{200D}'        // ZERO WIDTH JOINER
        | '\u{2028}'        // LINE SEPARATOR
        | '\u{2029}'        // PARAGRAPH SEPARATOR
        | '\u{2060}'        // WORD JOINER
        | '\u{2061}'..='\u{2063}' // Function application, invisible times, invisible separator
        | '\u{206A}'..='\u{206F}' // Various format control characters
        | '\u{FEFF}'        // ZERO WIDTH NO-BREAK SPACE
        | '\u{FFF9}'..='\u{FFFC}' // Interlinear annotation
        | '\u{1D173}'..='\u{1D17A}' // Musical formatting
    )
}

/// RFC 3454 Table C.3: Private use characters.
fn in_table_c3(c: char) -> bool {
    matches!(
        c,
        '\u{E000}'..='\u{F8FF}'       // Private Use Area
        | '\u{F0000}'..='\u{FFFFD}'   // Supplementary Private Use Area-A
        | '\u{100000}'..='\u{10FFFD}' // Supplementary Private Use Area-B
    )
}

/// RFC 3454 Table C.4: Non-character code points.
fn in_table_c4(c: char) -> bool {
    matches!(
        c,
        '\u{FDD0}'..='\u{FDEF}'
        | '\u{FFFE}'..='\u{FFFF}'
        | '\u{1FFFE}'..='\u{1FFFF}'
        | '\u{2FFFE}'..='\u{2FFFF}'
        | '\u{3FFFE}'..='\u{3FFFF}'
        | '\u{4FFFE}'..='\u{4FFFF}'
        | '\u{5FFFE}'..='\u{5FFFF}'
        | '\u{6FFFE}'..='\u{6FFFF}'
        | '\u{7FFFE}'..='\u{7FFFF}'
        | '\u{8FFFE}'..='\u{8FFFF}'
        | '\u{9FFFE}'..='\u{9FFFF}'
        | '\u{AFFFE}'..='\u{AFFFF}'
        | '\u{BFFFE}'..='\u{BFFFF}'
        | '\u{CFFFE}'..='\u{CFFFF}'
        | '\u{DFFFE}'..='\u{DFFFF}'
        | '\u{EFFFE}'..='\u{EFFFF}'
        | '\u{FFFFE}'..='\u{FFFFF}'
        | '\u{10FFFE}'..='\u{10FFFF}'
    )
}

/// RFC 3454 Table C.5: Surrogate codes (not applicable in Rust - handled by char).
fn in_table_c5(_c: char) -> bool {
    // Rust's char type cannot represent surrogate code points
    false
}

/// RFC 3454 Table C.6: Inappropriate for plain text.
fn in_table_c6(c: char) -> bool {
    matches!(
        c,
        '\u{FFF9}'          // INTERLINEAR ANNOTATION ANCHOR
        | '\u{FFFA}'        // INTERLINEAR ANNOTATION SEPARATOR
        | '\u{FFFB}'        // INTERLINEAR ANNOTATION TERMINATOR
        | '\u{FFFC}'        // OBJECT REPLACEMENT CHARACTER
        | '\u{FFFD}' // REPLACEMENT CHARACTER
    )
}

/// RFC 3454 Table C.7: Inappropriate for canonical representation.
fn in_table_c7(c: char) -> bool {
    matches!(c, '\u{2FF0}'..='\u{2FFB}') // Ideographic Description Characters
}

/// RFC 3454 Table C.8: Change display properties or deprecated.
fn in_table_c8(c: char) -> bool {
    matches!(
        c,
        '\u{0340}'          // COMBINING GRAVE TONE MARK
        | '\u{0341}'        // COMBINING ACUTE TONE MARK
        | '\u{200E}'        // LEFT-TO-RIGHT MARK
        | '\u{200F}'        // RIGHT-TO-LEFT MARK
        | '\u{202A}'        // LEFT-TO-RIGHT EMBEDDING
        | '\u{202B}'        // RIGHT-TO-LEFT EMBEDDING
        | '\u{202C}'        // POP DIRECTIONAL FORMATTING
        | '\u{202D}'        // LEFT-TO-RIGHT OVERRIDE
        | '\u{202E}'        // RIGHT-TO-LEFT OVERRIDE
        | '\u{206A}'        // INHIBIT SYMMETRIC SWAPPING
        | '\u{206B}'        // ACTIVATE SYMMETRIC SWAPPING
        | '\u{206C}'        // INHIBIT ARABIC FORM SHAPING
        | '\u{206D}'        // ACTIVATE ARABIC FORM SHAPING
        | '\u{206E}'        // NATIONAL DIGIT SHAPES
        | '\u{206F}' // NOMINAL DIGIT SHAPES
    )
}

/// RFC 3454 Table C.9: Tagging characters.
fn in_table_c9(c: char) -> bool {
    matches!(c, '\u{E0001}' | '\u{E0020}'..='\u{E007F}')
}

/// RFC 3454 Table A.1: Unassigned code points in Unicode 3.2.
fn in_table_a1(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        0x0221
        | 0x0234..=0x024F
        | 0x02AE..=0x02AF
        | 0x02EF..=0x02FF
        | 0x0350..=0x035F
        | 0x0370..=0x0373
        | 0x0376..=0x0379
        | 0x037B..=0x037D
        | 0x037F..=0x0383
        | 0x038B
        | 0x038D
        | 0x03A2
        | 0x03CF
        | 0x03F7..=0x03FF
        | 0x0487
        | 0x04CF
        | 0x04F6..=0x04F7
        | 0x04FA..=0x04FF
        | 0x0510..=0x0530
        | 0x0557..=0x0558
        | 0x0560
        | 0x0588
        | 0x058B..=0x0590
        | 0x05A2
        | 0x05BA
        | 0x05C5..=0x05CF
        | 0x05EB..=0x05EF
        | 0x05F5..=0x060B
        | 0x060D..=0x061A
        | 0x061C..=0x061E
        | 0x0620
        | 0x063B..=0x063F
        | 0x0656..=0x065F
        | 0x06EE..=0x06EF
        | 0x06FF
        | 0x070E
        | 0x072D..=0x072F
        | 0x074B..=0x077F
        | 0x07B2..=0x0900
        | 0x0904
        | 0x093A..=0x093B
        | 0x094E..=0x094F
        | 0x0955..=0x0957
        | 0x0971..=0x0980
        | 0x0984
        | 0x098D..=0x098E
        | 0x0991..=0x0992
        | 0x09A9
        | 0x09B1
        | 0x09B3..=0x09B5
        | 0x09BA..=0x09BB
        | 0x09BD
        | 0x09C5..=0x09C6
        | 0x09C9..=0x09CA
        | 0x09CE..=0x09D6
        | 0x09D8..=0x09DB
        | 0x09DE
        | 0x09E4..=0x09E5
        | 0x09FB..=0x0A01
        | 0x0A03..=0x0A04
        | 0x0A0B..=0x0A0E
        | 0x0A11..=0x0A12
        | 0x0A29
        | 0x0A31
        | 0x0A34
        | 0x0A37
        | 0x0A3A..=0x0A3B
        | 0x0A3D
        | 0x0A43..=0x0A46
        | 0x0A49..=0x0A4A
        | 0x0A4E..=0x0A58
        | 0x0A5D
        | 0x0A5F..=0x0A65
        | 0x0A75..=0x0A80
        | 0x0A84
        | 0x0A8C
        | 0x0A8E
        | 0x0A92
        | 0x0AA9
        | 0x0AB1
        | 0x0AB4
        | 0x0ABA..=0x0ABB
        | 0x0AC6
        | 0x0ACA
        | 0x0ACE..=0x0ACF
        | 0x0AD1..=0x0ADF
        | 0x0AE1..=0x0AE5
        | 0x0AF0..=0x0B00
        | 0x0B04
        | 0x0B0D..=0x0B0E
        | 0x0B11..=0x0B12
        | 0x0B29
        | 0x0B31
        | 0x0B34..=0x0B35
        | 0x0B3A..=0x0B3B
        | 0x0B44..=0x0B46
        | 0x0B49..=0x0B4A
        | 0x0B4E..=0x0B55
        | 0x0B58..=0x0B5B
        | 0x0B5E
        | 0x0B62..=0x0B65
        | 0x0B71..=0x0B81
        | 0x0B84
        | 0x0B8B..=0x0B8D
        | 0x0B91
        | 0x0B96..=0x0B98
        | 0x0B9B
        | 0x0B9D
        | 0x0BA0..=0x0BA2
        | 0x0BA5..=0x0BA7
        | 0x0BAB..=0x0BAD
        | 0x0BB6
        | 0x0BBA..=0x0BBD
        | 0x0BC3..=0x0BC5
        | 0x0BC9
        | 0x0BCE..=0x0BD6
        | 0x0BD8..=0x0BE6
        | 0x0BF3..=0x0C00
        | 0x0C04
        | 0x0C0D
        | 0x0C11
        | 0x0C29
        | 0x0C34
        | 0x0C3A..=0x0C3D
        | 0x0C45
        | 0x0C49
        | 0x0C4E..=0x0C54
        | 0x0C57..=0x0C5F
        | 0x0C62..=0x0C65
        | 0x0C70..=0x0C81
        | 0x0C84
        | 0x0C8D
        | 0x0C91
        | 0x0CA9
        | 0x0CB4
        | 0x0CBA..=0x0CBD
        | 0x0CC5
        | 0x0CC9
        | 0x0CCE..=0x0CD4
        | 0x0CD7..=0x0CDD
        | 0x0CDF
        | 0x0CE2..=0x0CE5
        | 0x0CF0..=0x0D01
        | 0x0D04
        | 0x0D0D
        | 0x0D11
        | 0x0D29
        | 0x0D3A..=0x0D3D
        | 0x0D44..=0x0D45
        | 0x0D49
        | 0x0D4E..=0x0D56
        | 0x0D58..=0x0D5F
        | 0x0D62..=0x0D65
        | 0x0D70..=0x0D81
        | 0x0D84
        | 0x0D97..=0x0D99
        | 0x0DB2
        | 0x0DBC
        | 0x0DBE..=0x0DBF
        | 0x0DC7..=0x0DC9
        | 0x0DCB..=0x0DCE
        | 0x0DD5
        | 0x0DD7
        | 0x0DE0..=0x0DF1
        | 0x0DF5..=0x0E00
        | 0x0E3B..=0x0E3E
        | 0x0E5C..=0x0E80
        | 0x0E83
        | 0x0E85..=0x0E86
        | 0x0E89
        | 0x0E8B..=0x0E8C
        | 0x0E8E..=0x0E93
        | 0x0E98
        | 0x0EA0
        | 0x0EA4
        | 0x0EA6
        | 0x0EA8..=0x0EA9
        | 0x0EAC
        | 0x0EBA
        | 0x0EBE..=0x0EBF
        | 0x0EC5
        | 0x0EC7
        | 0x0ECE..=0x0ECF
        | 0x0EDA..=0x0EDB
        | 0x0EDE..=0x0EFF
        | 0x0F48
        | 0x0F6B..=0x0F70
        | 0x0F8C..=0x0F8F
        | 0x0F98
        | 0x0FBD
        | 0x0FCD..=0x0FCE
        | 0x0FD0..=0x0FFF
        | 0x1022
        | 0x1028
        | 0x102B
        | 0x1033..=0x1035
        | 0x103A..=0x103F
        | 0x105A..=0x109F
        | 0x10C6..=0x10CF
        | 0x10F9..=0x10FA
        | 0x10FC..=0x10FF
        | 0x115A..=0x115E
        | 0x11A3..=0x11A7
        | 0x11FA..=0x11FF
        | 0x1207
        | 0x1247
        | 0x1249
        | 0x124E..=0x124F
        | 0x1257
        | 0x1259
        | 0x125E..=0x125F
        | 0x1287
        | 0x1289
        | 0x128E..=0x128F
        | 0x12AF
        | 0x12B1
        | 0x12B6..=0x12B7
        | 0x12BF
        | 0x12C1
        | 0x12C6..=0x12C7
        | 0x12CF
        | 0x12D7
        | 0x12EF
        | 0x130F
        | 0x1311
        | 0x1316..=0x1317
        | 0x131F
        | 0x1347
        | 0x135B..=0x1360
        | 0x137D..=0x139F
        | 0x13F5..=0x1400
        | 0x1677..=0x167F
        | 0x169D..=0x169F
        | 0x16F1..=0x16FF
        | 0x170D
        | 0x1715..=0x171F
        | 0x1737..=0x173F
        | 0x1754..=0x175F
        | 0x176D
        | 0x1771
        | 0x1774..=0x177F
        | 0x17DD..=0x17DF
        | 0x17EA..=0x17FF
        | 0x180F
        | 0x181A..=0x181F
        | 0x1878..=0x187F
        | 0x18AA..=0x1DFF
        | 0x1E9C..=0x1E9F
        | 0x1EFA..=0x1EFF
        | 0x1F16..=0x1F17
        | 0x1F1E..=0x1F1F
        | 0x1F46..=0x1F47
        | 0x1F4E..=0x1F4F
        | 0x1F58
        | 0x1F5A
        | 0x1F5C
        | 0x1F5E
        | 0x1F7E..=0x1F7F
        | 0x1FB5
        | 0x1FC5
        | 0x1FD4..=0x1FD5
        | 0x1FDC
        | 0x1FF0..=0x1FF1
        | 0x1FF5
        | 0x1FFF
        | 0x2053..=0x2056
        | 0x2058..=0x205E
        | 0x2064..=0x2069
        | 0x2072..=0x2073
        | 0x208F..=0x209F
        | 0x20B2..=0x20CF
        | 0x20EB..=0x20FF
        | 0x213B..=0x213C
        | 0x214C..=0x2152
        | 0x2184..=0x218F
        | 0x23CF..=0x23FF
        | 0x2427..=0x243F
        | 0x244B..=0x245F
        | 0x24FF
        | 0x2614..=0x2615
        | 0x2618
        | 0x267E..=0x267F
        | 0x268A..=0x2700
        | 0x2705
        | 0x270A..=0x270B
        | 0x2728
        | 0x274C
        | 0x274E
        | 0x2753..=0x2755
        | 0x2757
        | 0x275F..=0x2760
        | 0x2795..=0x2797
        | 0x27B0
        | 0x27BF..=0x27CF
        | 0x27EC..=0x27EF
        | 0x2B00..=0x2E7F
        | 0x2E9A
        | 0x2EF4..=0x2EFF
        | 0x2FD6..=0x2FEF
        | 0x2FFC..=0x2FFF
        | 0x3040
        | 0x3097..=0x3098
        | 0x3100..=0x3104
        | 0x312D..=0x3130
        | 0x318F
        | 0x31B8..=0x31EF
        | 0x321D..=0x321F
        | 0x3244..=0x3250
        | 0x327C..=0x327E
        | 0x32CC..=0x32CF
        | 0x32FF
        | 0x3377..=0x337A
        | 0x33DE..=0x33DF
        | 0x33FF
        | 0x4DB6..=0x4DFF
        | 0x9FA6..=0x9FFF
        | 0xA48D..=0xA48F
        | 0xA4C7..=0xABFF
        | 0xD7A4..=0xD7FF
        | 0xFA2E..=0xFA2F
        | 0xFA6B..=0xFAFF
        | 0xFB07..=0xFB12
        | 0xFB18..=0xFB1C
        | 0xFB37
        | 0xFB3D
        | 0xFB3F
        | 0xFB42
        | 0xFB45
        | 0xFBB2..=0xFBD2
        | 0xFD40..=0xFD4F
        | 0xFD90..=0xFD91
        | 0xFDC8..=0xFDCF
        | 0xFDFD..=0xFDFF
        | 0xFE10..=0xFE1F
        | 0xFE24..=0xFE2F
        | 0xFE47..=0xFE48
        | 0xFE53
        | 0xFE67
        | 0xFE6C..=0xFE6F
        | 0xFE75
        | 0xFEFD..=0xFEFE
        | 0xFF00
        | 0xFFBF..=0xFFC1
        | 0xFFC8..=0xFFC9
        | 0xFFD0..=0xFFD1
        | 0xFFD8..=0xFFD9
        | 0xFFDD..=0xFFDF
        | 0xFFE7
        | 0xFFEF..=0xFFF8
        | 0x10000..=0x102FF
        | 0x1031F
        | 0x10324..=0x1032F
        | 0x1034B..=0x103FF
        | 0x10426..=0x10427
        | 0x1044E..=0x1CFFF
        | 0x1D0F6..=0x1D0FF
        | 0x1D127..=0x1D129
        | 0x1D1DE..=0x1D3FF
        | 0x1D455
        | 0x1D49D
        | 0x1D4A0..=0x1D4A1
        | 0x1D4A3..=0x1D4A4
        | 0x1D4A7..=0x1D4A8
        | 0x1D4AD
        | 0x1D4BA
        | 0x1D4BC
        | 0x1D4C1
        | 0x1D4C4
        | 0x1D506
        | 0x1D50B..=0x1D50C
        | 0x1D515
        | 0x1D51D
        | 0x1D53A
        | 0x1D53F
        | 0x1D545
        | 0x1D547..=0x1D549
        | 0x1D551
        | 0x1D6A4..=0x1D6A7
        | 0x1D7CA..=0x1D7CD
        | 0x1D800..=0x1FFFD
        | 0x2A6D7..=0x2F7FF
        | 0x2FA1E..=0x2FFFD
        | 0x30000..=0x3FFFD
        | 0x40000..=0x4FFFD
        | 0x50000..=0x5FFFD
        | 0x60000..=0x6FFFD
        | 0x70000..=0x7FFFD
        | 0x80000..=0x8FFFD
        | 0x90000..=0x9FFFD
        | 0xA0000..=0xAFFFD
        | 0xB0000..=0xBFFFD
        | 0xC0000..=0xCFFFD
        | 0xD0000..=0xDFFFD
        | 0xE0000
        | 0xE0002..=0xE001F
        | 0xE0080..=0xEFFFD
    )
}

/// RFC 3454 Table D.1: Characters with bidirectional property "R" or "AL".
fn in_table_d1(c: char) -> bool {
    // This is a simplified version covering the main RTL ranges.
    // A complete implementation would check Unicode Bidi_Class property.
    matches!(
        c,
        '\u{05BE}'
        | '\u{05C0}'
        | '\u{05C3}'
        | '\u{05D0}'..='\u{05EA}'
        | '\u{05F0}'..='\u{05F4}'
        | '\u{061B}'
        | '\u{061F}'
        | '\u{0621}'..='\u{063A}'
        | '\u{0640}'..='\u{064A}'
        | '\u{066D}'..='\u{066F}'
        | '\u{0671}'..='\u{06D5}'
        | '\u{06DD}'
        | '\u{06E5}'..='\u{06E6}'
        | '\u{06FA}'..='\u{06FE}'
        | '\u{0700}'..='\u{070D}'
        | '\u{0710}'
        | '\u{0712}'..='\u{072C}'
        | '\u{0780}'..='\u{07A5}'
        | '\u{07B1}'
        | '\u{200F}'        // RIGHT-TO-LEFT MARK
        | '\u{FB1D}'
        | '\u{FB1F}'..='\u{FB28}'
        | '\u{FB2A}'..='\u{FB36}'
        | '\u{FB38}'..='\u{FB3C}'
        | '\u{FB3E}'
        | '\u{FB40}'..='\u{FB41}'
        | '\u{FB43}'..='\u{FB44}'
        | '\u{FB46}'..='\u{FBB1}'
        | '\u{FBD3}'..='\u{FD3D}'
        | '\u{FD50}'..='\u{FD8F}'
        | '\u{FD92}'..='\u{FDC7}'
        | '\u{FDF0}'..='\u{FDFC}'
        | '\u{FE70}'..='\u{FE74}'
        | '\u{FE76}'..='\u{FEFC}'
    )
}

/// RFC 3454 Table D.2: Characters with bidirectional property "L".
fn in_table_d2(c: char) -> bool {
    // Simplified: Check if character is in typical LTR ranges
    // Most Latin, Greek, Cyrillic, etc. characters have Bidi_Class L
    matches!(
        c,
        'A'..='Z'
        | 'a'..='z'
        | '\u{00C0}'..='\u{00D6}'
        | '\u{00D8}'..='\u{00F6}'
        | '\u{00F8}'..='\u{0220}'
        | '\u{0222}'..='\u{0233}'
        | '\u{0250}'..='\u{02AD}'
        | '\u{02B0}'..='\u{02B8}'
        | '\u{02BB}'..='\u{02C1}'
        | '\u{02D0}'..='\u{02D1}'
        | '\u{02E0}'..='\u{02E4}'
        | '\u{02EE}'
        | '\u{037A}'
        | '\u{0386}'
        | '\u{0388}'..='\u{038A}'
        | '\u{038C}'
        | '\u{038E}'..='\u{03A1}'
        | '\u{03A3}'..='\u{03CE}'
        | '\u{03D0}'..='\u{03F5}'
        | '\u{0400}'..='\u{0482}'
        | '\u{048A}'..='\u{04CE}'
        | '\u{04D0}'..='\u{04F5}'
        | '\u{04F8}'..='\u{04F9}'
        | '\u{0500}'..='\u{050F}'
        | '\u{0531}'..='\u{0556}'
        | '\u{0559}'..='\u{055F}'
        | '\u{0561}'..='\u{0587}'
        | '\u{0589}'
        | '\u{0903}'
        | '\u{0904}'..='\u{0939}'
        | '\u{093D}'
        | '\u{0940}'..='\u{0949}'
        | '\u{094B}'..='\u{094C}'
        | '\u{0950}'
    )
}

/// Check if character is prohibited according to RFC 4013 section 2.3.
fn is_prohibited(c: char, check_d1: bool, check_d2: bool, check_a1: bool) -> bool {
    in_table_c12(c)
        || in_table_c21(c)
        || in_table_c22(c)
        || in_table_c3(c)
        || in_table_c4(c)
        || in_table_c5(c)
        || in_table_c6(c)
        || in_table_c7(c)
        || in_table_c8(c)
        || in_table_c9(c)
        || (check_d1 && in_table_d1(c))
        || (check_d2 && in_table_d2(c))
        || (check_a1 && in_table_a1(c))
}

/// Prepare a string using the SASLprep profile of stringprep (RFC 4013).
///
/// # Arguments
/// * `data` - The string to SASLprep
/// * `prohibit_unassigned_code_points` - RFC 3454 and RFCs for various SASL mechanisms
///   distinguish between `queries` (unassigned code points allowed) and `stored strings`
///   (unassigned code points prohibited). When `true` (the default), unassigned code points
///   from Table A.1 are prohibited.
///
/// # Returns
/// The SASLprep'ed version of `data`, or an error if the string contains
/// prohibited characters or fails bidirectional checks.
pub fn saslprep(data: &str, prohibit_unassigned_code_points: bool) -> Result<String> {
    if data.is_empty() {
        return Ok(String::new());
    }

    // RFC3454 section 2, step 1 - Map
    // RFC4013 section 2.1 mappings:
    // - Map Non-ASCII space characters to SPACE (U+0020)
    // - Map commonly mapped to nothing characters to nothing
    let mapped: String = data
        .chars()
        .filter(|&c| !in_table_b1(c))
        .map(|c| if in_table_c12(c) { ' ' } else { c })
        .collect();

    if mapped.is_empty() {
        return Ok(String::new());
    }

    // RFC3454 section 2, step 2 - Normalize
    // RFC4013 section 2.2: Apply NFKC normalization
    let normalized: String = mapped.nfkc().collect();

    if normalized.is_empty() {
        return Ok(String::new());
    }

    // Get first and last characters for bidi check
    let first_char = normalized.chars().next().unwrap();
    let last_char = normalized.chars().last().unwrap();

    // RFC3454 Section 6: Bidirectional text handling
    let (check_d1, check_d2) = if in_table_d1(first_char) {
        // If first char is RandALCat, last must also be RandALCat
        if !in_table_d1(last_char) {
            return Err(PdfError::SaslPrepError(
                "failed bidirectional check".to_string(),
            ));
        }
        // If RandALCat present, must not contain LCat characters
        (false, true)
    } else {
        // If first is not RandALCat, no other can be either
        (true, false)
    };

    // RFC3454 section 2, step 3 and 4 - Prohibit and check bidi
    for c in normalized.chars() {
        if is_prohibited(c, check_d1, check_d2, prohibit_unassigned_code_points) {
            return Err(PdfError::SaslPrepError(
                "failed prohibited character check".to_string(),
            ));
        }
    }

    Ok(normalized)
}
