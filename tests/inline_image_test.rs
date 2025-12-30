use bolivar::pdfdevice::PDFDevice;
use bolivar::pdfdocument::PDFDocument;
use bolivar::pdfinterp::{PDFPageInterpreter, PDFResourceManager};
use bolivar::pdfpage::PDFPage;
use bolivar::pdftypes::PDFStream;
use bolivar::utils::Matrix;

#[derive(Default)]
struct InlineCaptureDevice {
    images: usize,
}

impl PDFDevice for InlineCaptureDevice {
    fn set_ctm(&mut self, _ctm: Matrix) {}
    fn ctm(&self) -> Option<Matrix> {
        None
    }
    fn begin_page(&mut self, _pageid: u32, _mediabox: (f64, f64, f64, f64), _ctm: Matrix) {}
    fn end_page(&mut self, _pageid: u32) {}
    fn begin_figure(&mut self, _name: &str, _bbox: (f64, f64, f64, f64), _matrix: Matrix) {}
    fn end_figure(&mut self, _name: &str) {}
    fn render_image(&mut self, _name: &str, _stream: &PDFStream) {
        self.images += 1;
    }
}

#[test]
fn test_inline_image_is_emitted() {
    // Minimal PDF with inline image in content stream (BI/ID/EI)
    let pdf_bytes = b"%PDF-1.4\n1 0 obj<< /Type /Catalog /Pages 2 0 R >>endobj\n2 0 obj<< /Type /Pages /Kids [3 0 R] /Count 1 >>endobj\n3 0 obj<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources << >> /Contents 4 0 R >>endobj\n4 0 obj<< /Length 53 >>stream\nq\nBI /W 1 /H 1 /BPC 8 /CS /DeviceGray ID\n\x00EI\nQ\nendstream\nendobj\nxref\n0 5\n0000000000 65535 f \n0000000010 00000 n \n0000000060 00000 n \n0000000111 00000 n \n0000000215 00000 n \ntrailer<< /Size 5 /Root 1 0 R >>\nstartxref\n320\n%%EOF";

    let doc = PDFDocument::new(pdf_bytes, "").unwrap();
    let page = PDFPage::create_pages(&doc).next().unwrap().unwrap();

    let mut rsrc = PDFResourceManager::with_caching(true);
    let mut device = InlineCaptureDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrc, &mut device);

    interp.process_page(&page, Some(&doc));

    assert_eq!(device.images, 1);
}
