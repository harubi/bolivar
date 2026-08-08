package sa.ingenious.pdf;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Path;
import java.util.Iterator;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import java.util.function.Supplier;
import sa.ingenious.ffi.BolivarKt;
import sa.ingenious.ffi.NativePdfDocument;

public final class Document implements AutoCloseable, Iterable<PageSummary> {
  private final DocumentBackend backend;

  private Document(DocumentBackend backend) {
    this.backend = backend;
  }

  static Document fromBackend(DocumentBackend backend) {
    return new Document(backend);
  }

  public static String loadNativeLibrary() {
    return NativeLibrary.load();
  }

  public static String version() {
    loadNativeLibrary();
    return BolivarKt.bolivarVersion();
  }

  public static Document open(String path) throws PdfException {
    return open(path, new DocumentOptions());
  }

  public static Document open(String path, DocumentOptions options) throws PdfException {
    return translate(
        () -> {
          loadNativeLibrary();
          var nativeDocument =
              NativePdfDocument.Companion.fromPath(path, DocumentOptionsKt.toNative(options));
          return new Document(new NativeDocumentBackend(nativeDocument));
        });
  }

  public static Document open(Path path) throws PdfException {
    return open(path, new DocumentOptions());
  }

  public static Document open(Path path, DocumentOptions options) throws PdfException {
    return open(path.toString(), options);
  }

  public static Document open(File file) throws PdfException {
    return open(file, new DocumentOptions());
  }

  public static Document open(File file, DocumentOptions options) throws PdfException {
    return open(file.toPath(), options);
  }

  public static Document open(byte[] pdfData) throws PdfException {
    return open(pdfData, new DocumentOptions());
  }

  public static Document open(byte[] pdfData, DocumentOptions options) throws PdfException {
    return translate(
        () -> {
          loadNativeLibrary();
          var nativeDocument =
              NativePdfDocument.Companion.fromBytes(pdfData.clone(), DocumentOptionsKt.toNative(options));
          return new Document(new NativeDocumentBackend(nativeDocument));
        });
  }

  public static Document open(InputStream inputStream) throws PdfException {
    return open(inputStream, new DocumentOptions());
  }

  public static Document open(InputStream inputStream, DocumentOptions options) throws PdfException {
    return translate(
        () -> {
          try (inputStream) {
            return open(inputStream.readAllBytes(), options);
          } catch (IOException error) {
            throw new PdfException.IoError(error.getMessage(), error);
          }
        });
  }

  public static CompletableFuture<Document> openAsync(Path path) {
    return openAsync(path, new DocumentOptions(), null);
  }

  public static CompletableFuture<Document> openAsync(Path path, DocumentOptions options) {
    return openAsync(path, options, null);
  }

  public static CompletableFuture<Document> openAsync(
      Path path, DocumentOptions options, Executor executor) {
    return future(executor, () -> open(path, options));
  }

  public static CompletableFuture<Document> openAsync(String path) {
    return openAsync(path, new DocumentOptions(), null);
  }

  public static CompletableFuture<Document> openAsync(String path, DocumentOptions options) {
    return openAsync(path, options, null);
  }

  public static CompletableFuture<Document> openAsync(
      String path, DocumentOptions options, Executor executor) {
    return future(executor, () -> open(path, options));
  }

  public static CompletableFuture<Document> openAsync(byte[] pdfData) {
    return openAsync(pdfData, new DocumentOptions(), null);
  }

  public static CompletableFuture<Document> openAsync(byte[] pdfData, DocumentOptions options) {
    return openAsync(pdfData, options, null);
  }

  public static CompletableFuture<Document> openAsync(
      byte[] pdfData, DocumentOptions options, Executor executor) {
    return future(executor, () -> open(pdfData, options));
  }

  public static String extractText(String path) throws PdfException {
    return extractText(path, new DocumentOptions());
  }

  public static String extractText(String path, DocumentOptions options) throws PdfException {
    try (var document = open(path, options)) {
      return document.extractText();
    }
  }

  public static String extractText(Path path) throws PdfException {
    return extractText(path, new DocumentOptions());
  }

  public static String extractText(Path path, DocumentOptions options) throws PdfException {
    try (var document = open(path, options)) {
      return document.extractText();
    }
  }

  public static String extractText(File file) throws PdfException {
    return extractText(file, new DocumentOptions());
  }

  public static String extractText(File file, DocumentOptions options) throws PdfException {
    try (var document = open(file, options)) {
      return document.extractText();
    }
  }

  public static String extractText(byte[] pdfData) throws PdfException {
    return extractText(pdfData, new DocumentOptions());
  }

  public static String extractText(byte[] pdfData, DocumentOptions options) throws PdfException {
    try (var document = open(pdfData, options)) {
      return document.extractText();
    }
  }

  public static String extractText(InputStream inputStream) throws PdfException {
    return extractText(inputStream, new DocumentOptions());
  }

  public static String extractText(InputStream inputStream, DocumentOptions options) throws PdfException {
    try (var document = open(inputStream, options)) {
      return document.extractText();
    }
  }

  public static CompletableFuture<String> extractTextAsync(Path path) {
    return extractTextAsync(path, new DocumentOptions(), null);
  }

  public static CompletableFuture<String> extractTextAsync(Path path, DocumentOptions options) {
    return extractTextAsync(path, options, null);
  }

  public static CompletableFuture<String> extractTextAsync(
      Path path, DocumentOptions options, Executor executor) {
    return future(executor, () -> extractText(path, options));
  }

  public static CompletableFuture<String> extractTextAsync(String path) {
    return extractTextAsync(path, new DocumentOptions(), null);
  }

  public static CompletableFuture<String> extractTextAsync(String path, DocumentOptions options) {
    return extractTextAsync(path, options, null);
  }

  public static CompletableFuture<String> extractTextAsync(
      String path, DocumentOptions options, Executor executor) {
    return future(executor, () -> extractText(path, options));
  }

  public static CompletableFuture<String> extractTextAsync(byte[] pdfData) {
    return extractTextAsync(pdfData, new DocumentOptions(), null);
  }

  public static CompletableFuture<String> extractTextAsync(byte[] pdfData, DocumentOptions options) {
    return extractTextAsync(pdfData, options, null);
  }

  public static CompletableFuture<String> extractTextAsync(
      byte[] pdfData, DocumentOptions options, Executor executor) {
    return future(executor, () -> extractText(pdfData, options));
  }

  public String extractText() throws PdfException {
    return translate(backend::extractText);
  }

  public CompletableFuture<String> extractTextAsync() {
    return extractTextAsync((Executor) null);
  }

  public CompletableFuture<String> extractTextAsync(Executor executor) {
    return future(executor, this::extractText);
  }

  public List<PageSummary> extractPageSummaries() throws PdfException {
    return translate(backend::extractPageSummaries);
  }

  public CompletableFuture<List<PageSummary>> extractPageSummariesAsync() {
    return extractPageSummariesAsync(null);
  }

  public CompletableFuture<List<PageSummary>> extractPageSummariesAsync(Executor executor) {
    return future(executor, this::extractPageSummaries);
  }

  public List<LayoutPage> extractLayoutPages() throws PdfException {
    return translate(backend::extractLayoutPages);
  }

  public CompletableFuture<List<LayoutPage>> extractLayoutPagesAsync() {
    return extractLayoutPagesAsync(null);
  }

  public CompletableFuture<List<LayoutPage>> extractLayoutPagesAsync(Executor executor) {
    return future(executor, this::extractLayoutPages);
  }

  public RawDocument extractRawDocument() throws PdfException {
    return translate(backend::extractRawDocument);
  }

  public RawPage extractRawPage(int pageNumber) throws PdfException {
    if (pageNumber <= 0) {
      throw new PdfException.InvalidArgument("pageNumber must be >= 1", null);
    }
    return translate(() -> backend.extractRawPage(pageNumber));
  }

  public RawDocumentMetadata metadata() throws PdfException {
    return translate(backend::metadata);
  }

  public List<Table> extractTables() throws PdfException {
    return translate(backend::extractTables);
  }

  public List<Table> extractTables(TableOptions options) throws PdfException {
    return translate(() -> backend.extractTables(options));
  }

  public List<PageTableRows> extractTableRows(TableOptions options) throws PdfException {
    return translate(() -> backend.extractTableRows(options));
  }

  public List<PageSummary> pages() throws PdfException {
    return extractPageSummaries();
  }

  public PageSummary get(int pageNumber) throws PdfException {
    if (pageNumber <= 0) {
      throw new PdfException.InvalidArgument("pageNumber must be >= 1", null);
    }
    return extractPageSummaries().stream()
        .filter(page -> page.pageNumber() == pageNumber)
        .findFirst()
        .orElseThrow(() -> new PdfException.InvalidArgument("Page " + pageNumber + " was not extracted", null));
  }

  @Override
  public Iterator<PageSummary> iterator() {
    return extractPageSummaries().iterator();
  }

  @Override
  public void close() {
    translate(
        () -> {
          backend.close();
          return null;
        });
  }

  private static <T> CompletableFuture<T> future(Executor executor, Supplier<T> supplier) {
    if (executor == null) {
      return CompletableFuture.supplyAsync(supplier);
    }
    return CompletableFuture.supplyAsync(supplier, executor);
  }

  private static <T> T translate(ThrowingSupplier<T> supplier) throws PdfException {
    try {
      return supplier.get();
    } catch (Throwable throwable) {
      throw PdfException.from(throwable);
    }
  }

  @FunctionalInterface
  private interface ThrowingSupplier<T> {
    T get() throws Throwable;
  }
}
