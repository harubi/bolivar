//! CCITT Fax decoder - Port of pdfminer.six ccitt.py
//!
//! ITU-T Recommendation T.4 - Group 3 facsimile
//! ITU-T Recommendation T.6 - Group 4 facsimile

use crate::{PdfError, Result};
use once_cell::sync::Lazy;

/// Huffman tree node - either a branch (left/right children) or a leaf value
#[derive(Clone, Debug)]
enum HuffNode {
    Branch(Box<HuffNode>, Box<HuffNode>),
    Leaf(HuffValue),
    Empty,
}

/// Value stored in Huffman leaf nodes
#[derive(Clone, Debug)]
enum HuffValue {
    Int(i32),
    Mode(Mode),
    Uncompressed(String),
}

/// CCITT mode codes
#[derive(Clone, Debug, PartialEq)]
enum Mode {
    Vertical(i32),
    Horizontal,
    Pass,
    Uncompressed,
    Extension(u8),
    Eofb,
}

impl HuffNode {
    fn new() -> Self {
        HuffNode::Empty
    }

    fn add(root: &mut HuffNode, value: HuffValue, bits: &str) {
        let mut current = root;
        for (i, c) in bits.chars().enumerate() {
            let is_last = i == bits.len() - 1;
            let bit = c == '1';

            if is_last {
                match current {
                    HuffNode::Empty => {
                        if bit {
                            *current = HuffNode::Branch(
                                Box::new(HuffNode::Empty),
                                Box::new(HuffNode::Leaf(value.clone())),
                            );
                        } else {
                            *current = HuffNode::Branch(
                                Box::new(HuffNode::Leaf(value.clone())),
                                Box::new(HuffNode::Empty),
                            );
                        }
                    }
                    HuffNode::Branch(left, right) => {
                        if bit {
                            **right = HuffNode::Leaf(value.clone());
                        } else {
                            **left = HuffNode::Leaf(value.clone());
                        }
                    }
                    HuffNode::Leaf(_) => panic!("Conflicting Huffman codes"),
                }
            } else {
                match current {
                    HuffNode::Empty => {
                        *current =
                            HuffNode::Branch(Box::new(HuffNode::Empty), Box::new(HuffNode::Empty));
                        if let HuffNode::Branch(left, right) = current {
                            current = if bit { right } else { left };
                        }
                    }
                    HuffNode::Branch(left, right) => {
                        current = if bit { right } else { left };
                    }
                    HuffNode::Leaf(_) => panic!("Conflicting Huffman codes"),
                }
            }
        }
    }
}

/// Build the MODE Huffman tree
fn build_mode_tree() -> HuffNode {
    let mut root = HuffNode::new();
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Vertical(0)), "1");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Vertical(1)), "011");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Vertical(-1)), "010");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Horizontal), "001");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Pass), "0001");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Vertical(2)), "000011");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Vertical(-2)), "000010");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Vertical(3)), "0000011");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Vertical(-3)), "0000010");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Uncompressed), "0000001111");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Extension(1)), "0000001000");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Extension(2)), "0000001001");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Extension(3)), "0000001010");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Extension(4)), "0000001011");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Extension(5)), "0000001100");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Extension(6)), "0000001101");
    HuffNode::add(&mut root, HuffValue::Mode(Mode::Extension(7)), "0000001110");
    HuffNode::add(
        &mut root,
        HuffValue::Mode(Mode::Eofb),
        "000000000001000000000001",
    );
    root
}

/// Build the WHITE run-length Huffman tree
fn build_white_tree() -> HuffNode {
    let mut root = HuffNode::new();
    HuffNode::add(&mut root, HuffValue::Int(0), "00110101");
    HuffNode::add(&mut root, HuffValue::Int(1), "000111");
    HuffNode::add(&mut root, HuffValue::Int(2), "0111");
    HuffNode::add(&mut root, HuffValue::Int(3), "1000");
    HuffNode::add(&mut root, HuffValue::Int(4), "1011");
    HuffNode::add(&mut root, HuffValue::Int(5), "1100");
    HuffNode::add(&mut root, HuffValue::Int(6), "1110");
    HuffNode::add(&mut root, HuffValue::Int(7), "1111");
    HuffNode::add(&mut root, HuffValue::Int(8), "10011");
    HuffNode::add(&mut root, HuffValue::Int(9), "10100");
    HuffNode::add(&mut root, HuffValue::Int(10), "00111");
    HuffNode::add(&mut root, HuffValue::Int(11), "01000");
    HuffNode::add(&mut root, HuffValue::Int(12), "001000");
    HuffNode::add(&mut root, HuffValue::Int(13), "000011");
    HuffNode::add(&mut root, HuffValue::Int(14), "110100");
    HuffNode::add(&mut root, HuffValue::Int(15), "110101");
    HuffNode::add(&mut root, HuffValue::Int(16), "101010");
    HuffNode::add(&mut root, HuffValue::Int(17), "101011");
    HuffNode::add(&mut root, HuffValue::Int(18), "0100111");
    HuffNode::add(&mut root, HuffValue::Int(19), "0001100");
    HuffNode::add(&mut root, HuffValue::Int(20), "0001000");
    HuffNode::add(&mut root, HuffValue::Int(21), "0010111");
    HuffNode::add(&mut root, HuffValue::Int(22), "0000011");
    HuffNode::add(&mut root, HuffValue::Int(23), "0000100");
    HuffNode::add(&mut root, HuffValue::Int(24), "0101000");
    HuffNode::add(&mut root, HuffValue::Int(25), "0101011");
    HuffNode::add(&mut root, HuffValue::Int(26), "0010011");
    HuffNode::add(&mut root, HuffValue::Int(27), "0100100");
    HuffNode::add(&mut root, HuffValue::Int(28), "0011000");
    HuffNode::add(&mut root, HuffValue::Int(29), "00000010");
    HuffNode::add(&mut root, HuffValue::Int(30), "00000011");
    HuffNode::add(&mut root, HuffValue::Int(31), "00011010");
    HuffNode::add(&mut root, HuffValue::Int(32), "00011011");
    HuffNode::add(&mut root, HuffValue::Int(33), "00010010");
    HuffNode::add(&mut root, HuffValue::Int(34), "00010011");
    HuffNode::add(&mut root, HuffValue::Int(35), "00010100");
    HuffNode::add(&mut root, HuffValue::Int(36), "00010101");
    HuffNode::add(&mut root, HuffValue::Int(37), "00010110");
    HuffNode::add(&mut root, HuffValue::Int(38), "00010111");
    HuffNode::add(&mut root, HuffValue::Int(39), "00101000");
    HuffNode::add(&mut root, HuffValue::Int(40), "00101001");
    HuffNode::add(&mut root, HuffValue::Int(41), "00101010");
    HuffNode::add(&mut root, HuffValue::Int(42), "00101011");
    HuffNode::add(&mut root, HuffValue::Int(43), "00101100");
    HuffNode::add(&mut root, HuffValue::Int(44), "00101101");
    HuffNode::add(&mut root, HuffValue::Int(45), "00000100");
    HuffNode::add(&mut root, HuffValue::Int(46), "00000101");
    HuffNode::add(&mut root, HuffValue::Int(47), "00001010");
    HuffNode::add(&mut root, HuffValue::Int(48), "00001011");
    HuffNode::add(&mut root, HuffValue::Int(49), "01010010");
    HuffNode::add(&mut root, HuffValue::Int(50), "01010011");
    HuffNode::add(&mut root, HuffValue::Int(51), "01010100");
    HuffNode::add(&mut root, HuffValue::Int(52), "01010101");
    HuffNode::add(&mut root, HuffValue::Int(53), "00100100");
    HuffNode::add(&mut root, HuffValue::Int(54), "00100101");
    HuffNode::add(&mut root, HuffValue::Int(55), "01011000");
    HuffNode::add(&mut root, HuffValue::Int(56), "01011001");
    HuffNode::add(&mut root, HuffValue::Int(57), "01011010");
    HuffNode::add(&mut root, HuffValue::Int(58), "01011011");
    HuffNode::add(&mut root, HuffValue::Int(59), "01001010");
    HuffNode::add(&mut root, HuffValue::Int(60), "01001011");
    HuffNode::add(&mut root, HuffValue::Int(61), "00110010");
    HuffNode::add(&mut root, HuffValue::Int(62), "00110011");
    HuffNode::add(&mut root, HuffValue::Int(63), "00110100");
    // Make-up codes
    HuffNode::add(&mut root, HuffValue::Int(64), "11011");
    HuffNode::add(&mut root, HuffValue::Int(128), "10010");
    HuffNode::add(&mut root, HuffValue::Int(192), "010111");
    HuffNode::add(&mut root, HuffValue::Int(256), "0110111");
    HuffNode::add(&mut root, HuffValue::Int(320), "00110110");
    HuffNode::add(&mut root, HuffValue::Int(384), "00110111");
    HuffNode::add(&mut root, HuffValue::Int(448), "01100100");
    HuffNode::add(&mut root, HuffValue::Int(512), "01100101");
    HuffNode::add(&mut root, HuffValue::Int(576), "01101000");
    HuffNode::add(&mut root, HuffValue::Int(640), "01100111");
    HuffNode::add(&mut root, HuffValue::Int(704), "011001100");
    HuffNode::add(&mut root, HuffValue::Int(768), "011001101");
    HuffNode::add(&mut root, HuffValue::Int(832), "011010010");
    HuffNode::add(&mut root, HuffValue::Int(896), "011010011");
    HuffNode::add(&mut root, HuffValue::Int(960), "011010100");
    HuffNode::add(&mut root, HuffValue::Int(1024), "011010101");
    HuffNode::add(&mut root, HuffValue::Int(1088), "011010110");
    HuffNode::add(&mut root, HuffValue::Int(1152), "011010111");
    HuffNode::add(&mut root, HuffValue::Int(1216), "011011000");
    HuffNode::add(&mut root, HuffValue::Int(1280), "011011001");
    HuffNode::add(&mut root, HuffValue::Int(1344), "011011010");
    HuffNode::add(&mut root, HuffValue::Int(1408), "011011011");
    HuffNode::add(&mut root, HuffValue::Int(1472), "010011000");
    HuffNode::add(&mut root, HuffValue::Int(1536), "010011001");
    HuffNode::add(&mut root, HuffValue::Int(1600), "010011010");
    HuffNode::add(&mut root, HuffValue::Int(1664), "011000");
    HuffNode::add(&mut root, HuffValue::Int(1728), "010011011");
    // Extended make-up codes (shared with black)
    HuffNode::add(&mut root, HuffValue::Int(1792), "00000001000");
    HuffNode::add(&mut root, HuffValue::Int(1856), "00000001100");
    HuffNode::add(&mut root, HuffValue::Int(1920), "00000001101");
    HuffNode::add(&mut root, HuffValue::Int(1984), "000000010010");
    HuffNode::add(&mut root, HuffValue::Int(2048), "000000010011");
    HuffNode::add(&mut root, HuffValue::Int(2112), "000000010100");
    HuffNode::add(&mut root, HuffValue::Int(2176), "000000010101");
    HuffNode::add(&mut root, HuffValue::Int(2240), "000000010110");
    HuffNode::add(&mut root, HuffValue::Int(2304), "000000010111");
    HuffNode::add(&mut root, HuffValue::Int(2368), "000000011100");
    HuffNode::add(&mut root, HuffValue::Int(2432), "000000011101");
    HuffNode::add(&mut root, HuffValue::Int(2496), "000000011110");
    HuffNode::add(&mut root, HuffValue::Int(2560), "000000011111");
    root
}

/// Build the BLACK run-length Huffman tree
fn build_black_tree() -> HuffNode {
    let mut root = HuffNode::new();
    HuffNode::add(&mut root, HuffValue::Int(0), "0000110111");
    HuffNode::add(&mut root, HuffValue::Int(1), "010");
    HuffNode::add(&mut root, HuffValue::Int(2), "11");
    HuffNode::add(&mut root, HuffValue::Int(3), "10");
    HuffNode::add(&mut root, HuffValue::Int(4), "011");
    HuffNode::add(&mut root, HuffValue::Int(5), "0011");
    HuffNode::add(&mut root, HuffValue::Int(6), "0010");
    HuffNode::add(&mut root, HuffValue::Int(7), "00011");
    HuffNode::add(&mut root, HuffValue::Int(8), "000101");
    HuffNode::add(&mut root, HuffValue::Int(9), "000100");
    HuffNode::add(&mut root, HuffValue::Int(10), "0000100");
    HuffNode::add(&mut root, HuffValue::Int(11), "0000101");
    HuffNode::add(&mut root, HuffValue::Int(12), "0000111");
    HuffNode::add(&mut root, HuffValue::Int(13), "00000100");
    HuffNode::add(&mut root, HuffValue::Int(14), "00000111");
    HuffNode::add(&mut root, HuffValue::Int(15), "000011000");
    HuffNode::add(&mut root, HuffValue::Int(16), "0000010111");
    HuffNode::add(&mut root, HuffValue::Int(17), "0000011000");
    HuffNode::add(&mut root, HuffValue::Int(18), "0000001000");
    HuffNode::add(&mut root, HuffValue::Int(19), "00001100111");
    HuffNode::add(&mut root, HuffValue::Int(20), "00001101000");
    HuffNode::add(&mut root, HuffValue::Int(21), "00001101100");
    HuffNode::add(&mut root, HuffValue::Int(22), "00000110111");
    HuffNode::add(&mut root, HuffValue::Int(23), "00000101000");
    HuffNode::add(&mut root, HuffValue::Int(24), "00000010111");
    HuffNode::add(&mut root, HuffValue::Int(25), "00000011000");
    HuffNode::add(&mut root, HuffValue::Int(26), "000011001010");
    HuffNode::add(&mut root, HuffValue::Int(27), "000011001011");
    HuffNode::add(&mut root, HuffValue::Int(28), "000011001100");
    HuffNode::add(&mut root, HuffValue::Int(29), "000011001101");
    HuffNode::add(&mut root, HuffValue::Int(30), "000001101000");
    HuffNode::add(&mut root, HuffValue::Int(31), "000001101001");
    HuffNode::add(&mut root, HuffValue::Int(32), "000001101010");
    HuffNode::add(&mut root, HuffValue::Int(33), "000001101011");
    HuffNode::add(&mut root, HuffValue::Int(34), "000011010010");
    HuffNode::add(&mut root, HuffValue::Int(35), "000011010011");
    HuffNode::add(&mut root, HuffValue::Int(36), "000011010100");
    HuffNode::add(&mut root, HuffValue::Int(37), "000011010101");
    HuffNode::add(&mut root, HuffValue::Int(38), "000011010110");
    HuffNode::add(&mut root, HuffValue::Int(39), "000011010111");
    HuffNode::add(&mut root, HuffValue::Int(40), "000001101100");
    HuffNode::add(&mut root, HuffValue::Int(41), "000001101101");
    HuffNode::add(&mut root, HuffValue::Int(42), "000011011010");
    HuffNode::add(&mut root, HuffValue::Int(43), "000011011011");
    HuffNode::add(&mut root, HuffValue::Int(44), "000001010100");
    HuffNode::add(&mut root, HuffValue::Int(45), "000001010101");
    HuffNode::add(&mut root, HuffValue::Int(46), "000001010110");
    HuffNode::add(&mut root, HuffValue::Int(47), "000001010111");
    HuffNode::add(&mut root, HuffValue::Int(48), "000001100100");
    HuffNode::add(&mut root, HuffValue::Int(49), "000001100101");
    HuffNode::add(&mut root, HuffValue::Int(50), "000001010010");
    HuffNode::add(&mut root, HuffValue::Int(51), "000001010011");
    HuffNode::add(&mut root, HuffValue::Int(52), "000000100100");
    HuffNode::add(&mut root, HuffValue::Int(53), "000000110111");
    HuffNode::add(&mut root, HuffValue::Int(54), "000000111000");
    HuffNode::add(&mut root, HuffValue::Int(55), "000000100111");
    HuffNode::add(&mut root, HuffValue::Int(56), "000000101000");
    HuffNode::add(&mut root, HuffValue::Int(57), "000001011000");
    HuffNode::add(&mut root, HuffValue::Int(58), "000001011001");
    HuffNode::add(&mut root, HuffValue::Int(59), "000000101011");
    HuffNode::add(&mut root, HuffValue::Int(60), "000000101100");
    HuffNode::add(&mut root, HuffValue::Int(61), "000001011010");
    HuffNode::add(&mut root, HuffValue::Int(62), "000001100110");
    HuffNode::add(&mut root, HuffValue::Int(63), "000001100111");
    // Make-up codes
    HuffNode::add(&mut root, HuffValue::Int(64), "0000001111");
    HuffNode::add(&mut root, HuffValue::Int(128), "000011001000");
    HuffNode::add(&mut root, HuffValue::Int(192), "000011001001");
    HuffNode::add(&mut root, HuffValue::Int(256), "000001011011");
    HuffNode::add(&mut root, HuffValue::Int(320), "000000110011");
    HuffNode::add(&mut root, HuffValue::Int(384), "000000110100");
    HuffNode::add(&mut root, HuffValue::Int(448), "000000110101");
    HuffNode::add(&mut root, HuffValue::Int(512), "0000001101100");
    HuffNode::add(&mut root, HuffValue::Int(576), "0000001101101");
    HuffNode::add(&mut root, HuffValue::Int(640), "0000001001010");
    HuffNode::add(&mut root, HuffValue::Int(704), "0000001001011");
    HuffNode::add(&mut root, HuffValue::Int(768), "0000001001100");
    HuffNode::add(&mut root, HuffValue::Int(832), "0000001001101");
    HuffNode::add(&mut root, HuffValue::Int(896), "0000001110010");
    HuffNode::add(&mut root, HuffValue::Int(960), "0000001110011");
    HuffNode::add(&mut root, HuffValue::Int(1024), "0000001110100");
    HuffNode::add(&mut root, HuffValue::Int(1088), "0000001110101");
    HuffNode::add(&mut root, HuffValue::Int(1152), "0000001110110");
    HuffNode::add(&mut root, HuffValue::Int(1216), "0000001110111");
    HuffNode::add(&mut root, HuffValue::Int(1280), "0000001010010");
    HuffNode::add(&mut root, HuffValue::Int(1344), "0000001010011");
    HuffNode::add(&mut root, HuffValue::Int(1408), "0000001010100");
    HuffNode::add(&mut root, HuffValue::Int(1472), "0000001010101");
    HuffNode::add(&mut root, HuffValue::Int(1536), "0000001011010");
    HuffNode::add(&mut root, HuffValue::Int(1600), "0000001011011");
    HuffNode::add(&mut root, HuffValue::Int(1664), "0000001100100");
    HuffNode::add(&mut root, HuffValue::Int(1728), "0000001100101");
    // Extended make-up codes (shared with white)
    HuffNode::add(&mut root, HuffValue::Int(1792), "00000001000");
    HuffNode::add(&mut root, HuffValue::Int(1856), "00000001100");
    HuffNode::add(&mut root, HuffValue::Int(1920), "00000001101");
    HuffNode::add(&mut root, HuffValue::Int(1984), "000000010010");
    HuffNode::add(&mut root, HuffValue::Int(2048), "000000010011");
    HuffNode::add(&mut root, HuffValue::Int(2112), "000000010100");
    HuffNode::add(&mut root, HuffValue::Int(2176), "000000010101");
    HuffNode::add(&mut root, HuffValue::Int(2240), "000000010110");
    HuffNode::add(&mut root, HuffValue::Int(2304), "000000010111");
    HuffNode::add(&mut root, HuffValue::Int(2368), "000000011100");
    HuffNode::add(&mut root, HuffValue::Int(2432), "000000011101");
    HuffNode::add(&mut root, HuffValue::Int(2496), "000000011110");
    HuffNode::add(&mut root, HuffValue::Int(2560), "000000011111");
    root
}

/// Build the UNCOMPRESSED mode Huffman tree
fn build_uncompressed_tree() -> HuffNode {
    let mut root = HuffNode::new();
    HuffNode::add(&mut root, HuffValue::Uncompressed("1".to_string()), "1");
    HuffNode::add(&mut root, HuffValue::Uncompressed("01".to_string()), "01");
    HuffNode::add(&mut root, HuffValue::Uncompressed("001".to_string()), "001");
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("0001".to_string()),
        "0001",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("00001".to_string()),
        "00001",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("00000".to_string()),
        "000001",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T00".to_string()),
        "00000011",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T10".to_string()),
        "00000010",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T000".to_string()),
        "000000011",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T100".to_string()),
        "000000010",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T0000".to_string()),
        "0000000011",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T1000".to_string()),
        "0000000010",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T00000".to_string()),
        "00000000011",
    );
    HuffNode::add(
        &mut root,
        HuffValue::Uncompressed("T10000".to_string()),
        "00000000010",
    );
    root
}

// Static Huffman trees - built once, shared by all parser instances
static MODE_TREE: Lazy<HuffNode> = Lazy::new(build_mode_tree);
static WHITE_TREE: Lazy<HuffNode> = Lazy::new(build_white_tree);
static BLACK_TREE: Lazy<HuffNode> = Lazy::new(build_black_tree);
static UNCOMPRESSED_TREE: Lazy<HuffNode> = Lazy::new(build_uncompressed_tree);

/// Parser state for handling different parsing phases
#[derive(Clone, Debug, PartialEq)]
enum ParseState {
    Mode,
    Horiz1,
    Horiz2,
    Uncompressed,
}

#[derive(Debug, PartialEq)]
enum ParseResult {
    Continue,
    Eofb,
    #[allow(dead_code)] // Reserved for byte-alignment mode
    ByteSkip,
    InvalidData,
}

/// A completed scanline ready for output
struct CompletedLine {
    y: usize,
    bits: Vec<i8>,
}

/// CCITT Group 4 Fax Parser
pub struct CCITTG4Parser {
    width: usize,
    #[allow(dead_code)] // Reserved for Group 3 fax byte-alignment mode
    bytealign: bool,
    curline: Vec<i8>,
    refline: Vec<i8>,
    curpos: isize,
    color: i8,
    y: usize,
    // Parsing state
    state: ParseState,
    huff_state: &'static HuffNode,
    n1: usize,
    n2: usize,
    // Completed lines queue (replaces inheritance-based output_line)
    completed_lines: Vec<CompletedLine>,
}

impl CCITTG4Parser {
    pub fn new(width: usize, bytealign: bool) -> Self {
        let mut parser = CCITTG4Parser {
            width,
            bytealign,
            curline: vec![1; width],
            refline: vec![1; width],
            curpos: -1,
            color: 1,
            y: 0,
            state: ParseState::Mode,
            huff_state: &*MODE_TREE,
            n1: 0,
            n2: 0,
            completed_lines: Vec::new(),
        };
        parser.reset();
        parser
    }

    pub fn set_curline(&mut self, line: Vec<i8>) {
        self.curline = line;
    }

    pub fn reset_line(&mut self) {
        self.refline = self.curline.clone();
        self.curline = vec![1; self.width];
        self.curpos = -1;
        self.color = 1;
    }

    pub fn reset(&mut self) {
        self.y = 0;
        self.curline = vec![1; self.width];
        self.reset_line();
        self.state = ParseState::Mode;
        self.huff_state = &*MODE_TREE;
        self.completed_lines.clear();
    }

    pub fn do_vertical(&mut self, dx: i32) {
        let mut x1 = (self.curpos + 1) as usize;

        // Find b1 - the first changing element on the reference line to the right of a0
        // that has the opposite color to a0
        loop {
            if x1 == 0 {
                if self.color == 1 && self.refline[x1] != self.color {
                    break;
                }
            } else if x1 >= self.refline.len()
                || (self.refline[x1 - 1] == self.color && self.refline[x1] != self.color)
            {
                break;
            }
            x1 += 1;
        }

        // Apply the vertical offset
        x1 = (x1 as i32 + dx) as usize;

        let x0 = 0.max(self.curpos) as usize;
        let x1_clamped = x1.min(self.width);

        // Fill the current line with the current color
        if x1_clamped < x0 {
            for x in x1_clamped..x0 {
                self.curline[x] = self.color;
            }
        } else {
            for x in x0..x1_clamped {
                self.curline[x] = self.color;
            }
        }

        self.curpos = x1_clamped as isize;
        self.color = 1 - self.color;
    }

    pub fn do_pass(&mut self) {
        let mut x1 = (self.curpos + 1) as usize;

        // Find b1
        loop {
            if x1 == 0 {
                if self.color == 1 && self.refline[x1] != self.color {
                    break;
                }
            } else if x1 >= self.refline.len()
                || (self.refline[x1 - 1] == self.color && self.refline[x1] != self.color)
            {
                break;
            }
            x1 += 1;
        }

        // Find b2 (next changing element after b1)
        loop {
            if x1 == 0 {
                if self.color == 0 && self.refline[x1] == self.color {
                    break;
                }
            } else if x1 >= self.refline.len()
                || (self.refline[x1 - 1] != self.color && self.refline[x1] == self.color)
            {
                break;
            }
            x1 += 1;
        }

        // Fill from curpos to x1 with current color
        let start = if self.curpos < 0 {
            0
        } else {
            self.curpos as usize
        };
        for x in start..x1 {
            if x < self.curline.len() {
                self.curline[x] = self.color;
            }
        }
        self.curpos = x1 as isize;
    }

    pub fn do_horizontal(&mut self, n1: usize, n2: usize) {
        if self.curpos < 0 {
            self.curpos = 0;
        }
        let mut x = self.curpos as usize;

        // First run with current color
        for _ in 0..n1 {
            if x >= self.curline.len() {
                break;
            }
            self.curline[x] = self.color;
            x += 1;
        }

        // Second run with opposite color
        for _ in 0..n2 {
            if x >= self.curline.len() {
                break;
            }
            self.curline[x] = 1 - self.color;
            x += 1;
        }

        self.curpos = x as isize;
    }

    fn do_uncompressed(&mut self, bits: &str) {
        for c in bits.chars() {
            if (self.curpos as usize) < self.curline.len() {
                self.curline[self.curpos as usize] = c.to_digit(10).unwrap() as i8;
                self.curpos += 1;
                self.flush_line();
            }
        }
    }

    pub fn curpos(&self) -> isize {
        self.curpos
    }

    pub fn color(&self) -> i8 {
        self.color
    }

    pub fn set_curpos(&mut self, pos: isize) {
        self.curpos = pos;
    }

    pub fn set_color(&mut self, color: i8) {
        self.color = color;
    }

    pub fn get_bits(&self) -> String {
        self.curline[..self.curpos as usize]
            .iter()
            .map(|b| char::from_digit(*b as u32, 10).unwrap())
            .collect()
    }

    fn flush_line(&mut self) -> bool {
        if self.curpos as usize >= self.width {
            // Queue the completed line instead of calling output_line directly
            self.completed_lines.push(CompletedLine {
                y: self.y,
                bits: self.curline.clone(),
            });
            self.y += 1;
            self.reset_line();
            return true;
        }
        false
    }

    /// Drain completed lines from the parser
    pub fn take_completed_lines(&mut self) -> Vec<(usize, Vec<i8>)> {
        self.completed_lines
            .drain(..)
            .map(|line| (line.y, line.bits))
            .collect()
    }

    fn parse_bit(&mut self, bit: bool) -> Option<ParseResult> {
        let next: &'static HuffNode = match self.huff_state {
            HuffNode::Branch(left, right) => {
                if bit {
                    &**right
                } else {
                    &**left
                }
            }
            _ => return None,
        };

        match next {
            HuffNode::Branch(_, _) => {
                self.huff_state = next;
                None
            }
            HuffNode::Leaf(value) => {
                let result = self.accept(value.clone());
                Some(result)
            }
            HuffNode::Empty => None,
        }
    }

    fn accept(&mut self, value: HuffValue) -> ParseResult {
        match self.state {
            ParseState::Mode => self.parse_mode(value),
            ParseState::Horiz1 => self.parse_horiz1(value),
            ParseState::Horiz2 => self.parse_horiz2(value),
            ParseState::Uncompressed => self.parse_uncompressed(value),
        }
    }

    fn parse_mode(&mut self, value: HuffValue) -> ParseResult {
        match value {
            HuffValue::Mode(mode) => match mode {
                Mode::Pass => {
                    self.do_pass();
                    self.flush_line();
                    self.huff_state = &*MODE_TREE;
                    ParseResult::Continue
                }
                Mode::Horizontal => {
                    self.n1 = 0;
                    self.state = ParseState::Horiz1;
                    self.huff_state = if self.color == 1 {
                        &*WHITE_TREE
                    } else {
                        &*BLACK_TREE
                    };
                    ParseResult::Continue
                }
                Mode::Uncompressed => {
                    self.state = ParseState::Uncompressed;
                    self.huff_state = &*UNCOMPRESSED_TREE;
                    ParseResult::Continue
                }
                Mode::Eofb => ParseResult::Eofb,
                Mode::Vertical(dx) => {
                    self.do_vertical(dx);
                    self.flush_line();
                    self.huff_state = &*MODE_TREE;
                    ParseResult::Continue
                }
                Mode::Extension(_) => ParseResult::InvalidData,
            },
            _ => ParseResult::InvalidData,
        }
    }

    fn parse_horiz1(&mut self, value: HuffValue) -> ParseResult {
        match value {
            HuffValue::Int(n) => {
                self.n1 += n as usize;
                if n < 64 {
                    self.n2 = 0;
                    self.color = 1 - self.color;
                    self.state = ParseState::Horiz2;
                }
                self.huff_state = if self.color == 1 {
                    &*WHITE_TREE
                } else {
                    &*BLACK_TREE
                };
                ParseResult::Continue
            }
            _ => ParseResult::InvalidData,
        }
    }

    fn parse_horiz2(&mut self, value: HuffValue) -> ParseResult {
        match value {
            HuffValue::Int(n) => {
                self.n2 += n as usize;
                if n < 64 {
                    self.color = 1 - self.color;
                    self.state = ParseState::Mode;
                    self.do_horizontal(self.n1, self.n2);
                    self.flush_line();
                    self.huff_state = &*MODE_TREE;
                } else {
                    self.huff_state = if self.color == 1 {
                        &*WHITE_TREE
                    } else {
                        &*BLACK_TREE
                    };
                }
                ParseResult::Continue
            }
            _ => ParseResult::InvalidData,
        }
    }

    fn parse_uncompressed(&mut self, value: HuffValue) -> ParseResult {
        match value {
            HuffValue::Uncompressed(bits) => {
                if bits.starts_with('T') {
                    self.state = ParseState::Mode;
                    self.color = bits.chars().nth(1).unwrap().to_digit(10).unwrap() as i8;
                    self.do_uncompressed(&bits[2..]);
                    self.huff_state = &*MODE_TREE;
                } else {
                    self.do_uncompressed(&bits);
                    self.huff_state = &*UNCOMPRESSED_TREE;
                }
                ParseResult::Continue
            }
            _ => ParseResult::InvalidData,
        }
    }

    pub fn feedbytes(&mut self, data: &[u8]) {
        for &byte in data {
            let mut should_break = false;
            for m in [128, 64, 32, 16, 8, 4, 2, 1] {
                let result = self.parse_bit((byte & m) != 0);
                match result {
                    Some(ParseResult::Eofb) => {
                        should_break = true;
                        break;
                    }
                    Some(ParseResult::ByteSkip) => {
                        self.state = ParseState::Mode;
                        self.huff_state = &*MODE_TREE;
                        break;
                    }
                    _ => {}
                }
            }
            if should_break {
                break;
            }
        }
    }
}

/// CCITT Fax Decoder - outputs decoded image data
///
/// This mirrors Python's CCITTFaxDecoder(CCITTG4Parser) inheritance pattern.
/// Instead of method override, we use the parser's completed_lines queue.
pub struct CCITTFaxDecoder {
    parser: CCITTG4Parser,
    reversed: bool,
    buf: Vec<u8>,
}

impl CCITTFaxDecoder {
    pub fn new(width: usize, bytealign: bool, reversed: bool) -> Self {
        CCITTFaxDecoder {
            parser: CCITTG4Parser::new(width, bytealign),
            reversed,
            buf: Vec::new(),
        }
    }

    /// Convert a scanline to packed bytes and append to buffer.
    /// This is equivalent to Python's output_line override.
    pub fn output_line(&mut self, _y: usize, bits: &[i8]) {
        let mut arr = vec![0u8; bits.len().div_ceil(8)];
        let bits_to_use: Vec<i8> = if self.reversed {
            bits.iter().map(|&b| 1 - b).collect()
        } else {
            bits.to_vec()
        };

        for (i, &b) in bits_to_use.iter().enumerate() {
            if b != 0 {
                arr[i / 8] += [128, 64, 32, 16, 8, 4, 2, 1][i % 8];
            }
        }
        self.buf.extend_from_slice(&arr);
    }

    pub fn close(&self) -> Vec<u8> {
        self.buf.clone()
    }

    pub fn feedbytes(&mut self, data: &[u8]) {
        // Feed data to the parser
        self.parser.feedbytes(data);

        // Process all completed lines from the parser
        // This replaces the Python inheritance-based output_line callback
        for (y, bits) in self.parser.take_completed_lines() {
            self.output_line(y, &bits);
        }
    }
}

/// Decode CCITT fax data
pub fn ccittfaxdecode(data: &[u8], params: &CcittParams) -> Result<Vec<u8>> {
    if params.k == -1 {
        let mut decoder =
            CCITTFaxDecoder::new(params.columns, params.encoded_byte_align, params.black_is_1);
        decoder.feedbytes(data);
        Ok(decoder.close())
    } else {
        Err(PdfError::DecodeError(format!(
            "Unsupported K value: {}",
            params.k
        )))
    }
}

/// CCITT decoding parameters
pub struct CcittParams {
    pub k: i32,
    pub columns: usize,
    pub encoded_byte_align: bool,
    pub black_is_1: bool,
}
