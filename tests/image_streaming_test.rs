use std::collections::HashMap;
use std::io::Write;
use std::time::SystemTime;

use bolivar::image::ImageWriter;
use bolivar::pdftypes::{PDFObject, PDFStream};
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
    let mut attrs = HashMap::new();
    attrs.insert(
        "Filter".to_string(),
        PDFObject::Name("FlateDecode".to_string()),
    );
    attrs.insert("Width".to_string(), PDFObject::Int(2));
    attrs.insert("Height".to_string(), PDFObject::Int(2));
    attrs.insert("BitsPerComponent".to_string(), PDFObject::Int(8));
    attrs.insert(
        "ColorSpace".to_string(),
        PDFObject::Name("DeviceGray".to_string()),
    );

    // 2x2 DeviceGray expects 4 bytes, but we compress 10 bytes.
    let raw = zlib_compress(&vec![0u8; 10]);
    let stream = PDFStream::new(attrs, raw);

    let outdir = make_temp_dir("bolivar_image_test");
    let mut writer = ImageWriter::new(&outdir).unwrap();
    let res = writer.export_image(
        "img",
        &stream,
        (Some(2), Some(2)),
        8,
        &vec!["DeviceGray".to_string()],
    );

    let _ = std::fs::remove_dir_all(&outdir);

    assert!(res.is_err());
}
