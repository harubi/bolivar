use std::io::Write;
use std::time::SystemTime;

use bolivar_core::image::ImageWriter;
use bolivar_core::pdftypes::{PDFDict, PDFObject, PDFStream};
use flate2::{Compression, write::ZlibEncoder};

fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("{}_{}", prefix, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn test_image_decode_rejects_oversize() {
    let mut attrs = PDFDict::default();
    attrs.insert("Filter".into(), PDFObject::Name("FlateDecode".into()));
    attrs.insert("Width".into(), PDFObject::Int(2));
    attrs.insert("Height".into(), PDFObject::Int(2));
    attrs.insert("BitsPerComponent".into(), PDFObject::Int(8));
    attrs.insert("ColorSpace".into(), PDFObject::Name("DeviceGray".into()));

    // 2x2 DeviceGray expects 4 bytes, but we compress 10 bytes.
    let raw = zlib_compress(&[0u8; 10]);
    let stream = PDFStream::new(attrs, raw);

    let outdir = make_temp_dir("bolivar_image_test");
    let mut writer = ImageWriter::new(&outdir).unwrap();
    let res = writer.export_image(
        "img",
        &stream,
        (Some(2), Some(2)),
        8,
        &["DeviceGray".into()],
    );

    let _ = std::fs::remove_dir_all(&outdir);

    assert!(res.is_err());
}
