package sa.ingenious;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.io.File;
import java.io.InputStream;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import org.junit.jupiter.api.Test;

class BolivarJavaCompileTest {
  private static Class<?> bolivarClass() throws Exception {
    return Class.forName("sa.ingenious.bolivar");
  }

  @Test
  void lowercaseBolivarEntrypointExists() throws Exception {
    Class<?> entrypoint = bolivarClass();
    Method open = entrypoint.getMethod("open", String.class);
    Method extractText = entrypoint.getMethod("extractText", String.class);

    assertEquals(PdfDocument.class, open.getReturnType());
    assertEquals(String.class, extractText.getReturnType());
  }

  @Test
  void bolivarOpenHasExpectedOverloads() throws Exception {
    Class<?> bolivar = bolivarClass();
    assertEquals(PdfDocument.class, bolivar.getMethod("open", String.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", String.class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", Path.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", Path.class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", File.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", File.class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", byte[].class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", byte[].class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", InputStream.class).getReturnType());
    assertEquals(PdfDocument.class, bolivar.getMethod("open", InputStream.class, DocumentOptions.class).getReturnType());
  }

  @Test
  void bolivarFutureEntryPointsReturnCompletableFuture() throws Exception {
    Class<?> bolivar = bolivarClass();
    Method openFuturePath = bolivar.getMethod("openFuture", String.class);
    Method openFuturePathWithOptions = bolivar.getMethod("openFuture", String.class, DocumentOptions.class);
    Method openFuturePathWithExecutor = bolivar.getMethod("openFuture", String.class, DocumentOptions.class, Executor.class);
    Method extractFuturePath = bolivar.getMethod("extractTextFuture", String.class);
    Method extractFutureBytes = bolivar.getMethod("extractTextFuture", byte[].class);

    assertEquals(CompletableFuture.class, openFuturePath.getReturnType());
    assertEquals(CompletableFuture.class, openFuturePathWithOptions.getReturnType());
    assertEquals(CompletableFuture.class, openFuturePathWithExecutor.getReturnType());
    assertEquals(CompletableFuture.class, extractFuturePath.getReturnType());
    assertEquals(CompletableFuture.class, extractFutureBytes.getReturnType());
  }

  @Test
  void bolivarHasNativeLoaderEntryPoint() throws Exception {
    Method method = bolivarClass().getMethod("loadNativeLibrary");
    assertEquals(String.class, method.getReturnType());
  }

  @Test
  void pdfDocumentFutureMethodsReturnCompletableFuture() throws Exception {
    Method textFuture = PdfDocument.class.getMethod("extractTextFuture");
    Method textFutureWithExecutor = PdfDocument.class.getMethod("extractTextFuture", Executor.class);
    Method summariesFuture = PdfDocument.class.getMethod("extractPageSummariesFuture");
    Method layoutFuture = PdfDocument.class.getMethod("extractLayoutPagesFuture");

    assertEquals(CompletableFuture.class, textFuture.getReturnType());
    assertEquals(CompletableFuture.class, textFutureWithExecutor.getReturnType());
    assertEquals(CompletableFuture.class, summariesFuture.getReturnType());
    assertEquals(CompletableFuture.class, layoutFuture.getReturnType());
  }

  @Test
  void pdfDocumentHasSingleTableEntrypoint() {
    assertThrows(NoSuchMethodException.class, () -> PdfDocument.class.getMethod("extractTablesFuture"));
    assertThrows(NoSuchMethodException.class, () -> PdfDocument.class.getMethod("tables"));
  }

  @Test
  void oldBolivarClassIsRemoved() {
    assertThrows(ClassNotFoundException.class, () -> Class.forName("sa.ingenious.bolivar.Bolivar"));
  }

  @Test
  void documentOptionsHasJavaBuilderEntryPoint() throws Exception {
    Method builderMethod = DocumentOptions.class.getMethod("builder");
    assertEquals(DocumentOptions.Builder.class, builderMethod.getReturnType());
  }
}
