package sa.ingenious.bolivar;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.io.File;
import java.io.InputStream;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import org.junit.jupiter.api.Test;

class BolivarJavaCompileTest {
  @Test
  void bolivarOpenHasExpectedOverloads() throws Exception {
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", String.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", String.class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", Path.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", Path.class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", File.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", File.class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", byte[].class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", byte[].class, DocumentOptions.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", InputStream.class).getReturnType());
    assertEquals(PdfDocument.class, Bolivar.class.getMethod("open", InputStream.class, DocumentOptions.class).getReturnType());
  }

  @Test
  void bolivarFutureEntryPointsReturnCompletableFuture() throws Exception {
    Method openFuturePath = Bolivar.class.getMethod("openFuture", String.class);
    Method openFuturePathWithOptions = Bolivar.class.getMethod("openFuture", String.class, DocumentOptions.class);
    Method openFuturePathWithExecutor = Bolivar.class.getMethod("openFuture", String.class, DocumentOptions.class, Executor.class);
    Method extractFuturePath = Bolivar.class.getMethod("extractTextFuture", String.class);
    Method extractFutureBytes = Bolivar.class.getMethod("extractTextFuture", byte[].class);

    assertEquals(CompletableFuture.class, openFuturePath.getReturnType());
    assertEquals(CompletableFuture.class, openFuturePathWithOptions.getReturnType());
    assertEquals(CompletableFuture.class, openFuturePathWithExecutor.getReturnType());
    assertEquals(CompletableFuture.class, extractFuturePath.getReturnType());
    assertEquals(CompletableFuture.class, extractFutureBytes.getReturnType());
  }

  @Test
  void bolivarHasNativeLoaderEntryPoint() throws Exception {
    Method method = Bolivar.class.getMethod("loadNativeLibrary");
    assertEquals(String.class, method.getReturnType());
  }

  @Test
  void pdfDocumentFutureMethodsReturnCompletableFuture() throws Exception {
    Method textFuture = PdfDocument.class.getMethod("extractTextFuture");
    Method textFutureWithExecutor = PdfDocument.class.getMethod("extractTextFuture", Executor.class);
    Method summariesFuture = PdfDocument.class.getMethod("extractPageSummariesFuture");
    Method layoutFuture = PdfDocument.class.getMethod("extractLayoutPagesFuture");
    Method tablesFuture = PdfDocument.class.getMethod("extractTablesFuture");

    assertEquals(CompletableFuture.class, textFuture.getReturnType());
    assertEquals(CompletableFuture.class, textFutureWithExecutor.getReturnType());
    assertEquals(CompletableFuture.class, summariesFuture.getReturnType());
    assertEquals(CompletableFuture.class, layoutFuture.getReturnType());
    assertEquals(CompletableFuture.class, tablesFuture.getReturnType());
  }

  @Test
  void documentOptionsHasJavaBuilderEntryPoint() throws Exception {
    Method builderMethod = DocumentOptions.class.getMethod("builder");
    assertEquals(DocumentOptions.Builder.class, builderMethod.getReturnType());
  }
}
