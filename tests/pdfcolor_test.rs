//! Tests for pdfcolor module.
//!
//! Note: pdfminer.six has no explicit tests for pdfcolor.
//! These tests verify the predefined colorspace registry.

use bolivar::pdfcolor::{PDFColorSpace, PREDEFINED_COLORSPACE};

#[test]
fn test_predefined_colorspaces() {
    // DeviceGray has 1 component
    let gray = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();
    assert_eq!(gray.name, "DeviceGray");
    assert_eq!(gray.ncomponents, 1);

    // DeviceRGB has 3 components
    let rgb = PREDEFINED_COLORSPACE.get("DeviceRGB").unwrap();
    assert_eq!(rgb.name, "DeviceRGB");
    assert_eq!(rgb.ncomponents, 3);

    // DeviceCMYK has 4 components
    let cmyk = PREDEFINED_COLORSPACE.get("DeviceCMYK").unwrap();
    assert_eq!(cmyk.name, "DeviceCMYK");
    assert_eq!(cmyk.ncomponents, 4);
}

#[test]
fn test_colorspace_new() {
    let cs = PDFColorSpace::new("Custom", 5);
    assert_eq!(cs.name, "Custom");
    assert_eq!(cs.ncomponents, 5);
}

#[test]
fn test_all_predefined_colorspaces() {
    // Verify all 9 predefined colorspaces exist
    let expected = [
        ("DeviceGray", 1),
        ("CalRGB", 3),
        ("CalGray", 1),
        ("Lab", 3),
        ("DeviceRGB", 3),
        ("DeviceCMYK", 4),
        ("Separation", 1),
        ("Indexed", 1),
        ("Pattern", 1),
    ];

    for (name, components) in expected {
        let cs = PREDEFINED_COLORSPACE
            .get(name)
            .unwrap_or_else(|| panic!("Missing colorspace: {}", name));
        assert_eq!(cs.ncomponents, components, "Wrong components for {}", name);
    }
}
