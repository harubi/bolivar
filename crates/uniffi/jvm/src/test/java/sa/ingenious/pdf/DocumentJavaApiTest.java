package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.File;
import java.io.InputStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import java.util.function.Consumer;
import org.junit.jupiter.api.Test;

class DocumentJavaApiTest {
  @Test
  void javaCallSiteUsesDomainPackageDocumentFactoriesAndFluentBuilders() throws Exception {
    DocumentOptions options =
        DocumentOptions.builder()
            .password("secret")
            .pages(1, 2)
            .maxPages(2)
            .caching(false)
            .bidi(true)
            .layout(layout -> layout.wordMargin(0.2).detectVertical(true))
            .build();

    assertEquals("secret", options.password());
    assertEquals(List.of(1, 2), options.pageNumbers());
    assertEquals(2, options.maxPages());
    assertFalse(options.caching());
    assertTrue(options.bidi());
    assertEquals(0.2, options.layout().wordMargin());
    assertTrue(options.layout().detectVertical());
    assertEquals(
        DocumentOptions.class,
        DocumentOptions.class
            .getConstructor(String.class, List.class, Integer.class, boolean.class, LayoutOptions.class)
            .getDeclaringClass());

    assertEquals(Document.class, Document.class.getMethod("open", Path.class).getReturnType());
    assertEquals(
        Document.class,
        Document.class.getMethod("open", Path.class, DocumentOptions.class).getReturnType());
    assertEquals(String.class, Document.class.getMethod("extractText", Path.class).getReturnType());
    assertEquals(
        String.class,
        Document.class.getMethod("extractText", Path.class, DocumentOptions.class).getReturnType());
    assertEquals(RawDocument.class, Document.class.getMethod("extractRawDocument").getReturnType());
    assertEquals(RawPage.class, Document.class.getMethod("extractRawPage", int.class).getReturnType());
    assertEquals(RawDocumentMetadata.class, Document.class.getMethod("metadata").getReturnType());
    assertEquals(String.class, Document.class.getMethod("version").getReturnType());
  }

  @Test
  void javaFactoriesCoverStandardInputTypes() throws Exception {
    assertEquals(Document.class, Document.class.getMethod("open", String.class).getReturnType());
    assertEquals(Document.class, Document.class.getMethod("open", File.class).getReturnType());
    assertEquals(Document.class, Document.class.getMethod("open", byte[].class).getReturnType());
    assertEquals(Document.class, Document.class.getMethod("open", InputStream.class).getReturnType());
  }

  @Test
  void javaAsyncUsesCompletableFutureAndExecutorOverloads() throws Exception {
    assertEquals(CompletableFuture.class, Document.class.getMethod("openAsync", Path.class).getReturnType());
    assertEquals(
        CompletableFuture.class,
        Document.class.getMethod("openAsync", Path.class, DocumentOptions.class).getReturnType());
    assertEquals(
        CompletableFuture.class,
        Document.class
            .getMethod("openAsync", Path.class, DocumentOptions.class, Executor.class)
            .getReturnType());
    assertEquals(CompletableFuture.class, Document.class.getMethod("extractTextAsync", Path.class).getReturnType());
    assertEquals(CompletableFuture.class, Document.class.getMethod("extractTextAsync").getReturnType());
  }

  @Test
  void publicModelsUseModernJavaRecordAccessors() throws Exception {
    BoundingBox bbox = new BoundingBox(1.0, 2.0, 3.0, 4.0);
    PageSummary summary = new PageSummary(1, "hello", bbox, 0.0);

    assertTrue(BoundingBox.class.isRecord());
    assertTrue(PageSummary.class.isRecord());
    assertTrue(RawDocument.class.isRecord());
    assertTrue(RawDocumentMetadata.class.isRecord());
    assertEquals(1.0, bbox.x0());
    assertEquals(1, summary.pageNumber());
    assertThrows(NoSuchMethodException.class, () -> PageSummary.class.getMethod("getPageNumber"));
  }

  @Test
  void javaBuilderDoesNotExposeKotlinFunctionTypes() throws Exception {
    Method layoutConsumer = DocumentOptions.Builder.class.getMethod("layout", Consumer.class);
    assertEquals(DocumentOptions.Builder.class, layoutConsumer.getReturnType());

    for (Method method : DocumentOptions.Builder.class.getMethods()) {
      assertNoPublicLeak(method.getReturnType());
      for (Class<?> parameterType : method.getParameterTypes()) {
        assertNoPublicLeak(parameterType);
      }
    }
  }

  @Test
  void publicDocumentApiDoesNotExposeGeneratedOrKotlinOnlyTypes() {
    for (Method method : Document.class.getMethods()) {
      if (!Modifier.isPublic(method.getModifiers()) || method.getDeclaringClass() == Object.class) {
        continue;
      }
      assertNoPublicLeak(method.getReturnType());
      for (Class<?> parameterType : method.getParameterTypes()) {
        assertNoPublicLeak(parameterType);
      }
    }

    for (Constructor<?> constructor : Document.class.getConstructors()) {
      for (Class<?> parameterType : constructor.getParameterTypes()) {
        assertNoPublicLeak(parameterType);
      }
    }
  }

  @Test
  void oldPublicNamesAreGone() {
    assertThrows(ClassNotFoundException.class, () -> Class.forName("sa.ingenious.bolivar"));
    assertThrows(ClassNotFoundException.class, () -> Class.forName("sa.ingenious.PdfDocument"));
    assertThrows(ClassNotFoundException.class, () -> Class.forName("sa.ingenious.BolivarClojureInterop"));
  }

  private static void assertNoPublicLeak(Class<?> type) {
    String name = type.getName();
    assertFalse(name.startsWith("sa.ingenious.ffi"), name);
    assertFalse(name.startsWith("kotlin.jvm.functions"), name);
    assertFalse(name.equals("kotlin.Unit"), name);
    assertFalse(name.equals("kotlin.UInt"), name);
  }
}
