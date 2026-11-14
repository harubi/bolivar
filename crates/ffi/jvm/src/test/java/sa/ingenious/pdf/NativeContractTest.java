package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;
import sa.ingenious.pdf.internal.Contract;
import sa.ingenious.pdf.internal.Native;
import sa.ingenious.pdf.internal.NativeLoader;
import sa.ingenious.pdf.internal.Schema;

/** The jar and the native library agree on versions, layouts, codes and column schemas. */
class NativeContractTest {
  @Test
  void versionsMatch() {
    Native.ensureLoaded();
    assertEquals(NativeLoader.jarVersion(), Document.version());
  }

  @Test
  void everySchemaMatchesNative() {
    Native.ensureLoaded();
    for (Schema.Kind kind : Schema.Kind.values()) {
      assertEquals(kind.expected(), Native.schema(kind.number()), kind.name());
    }
  }

  @Test
  void everyLayoutAndCodeMatchesNative() {
    Native.ensureLoaded();
    Contract.verify(Native::describe);
    assertEquals(96, Contract.OPTIONS.byteSize());
    assertEquals(184, Contract.TABLE_OPTIONS.byteSize());
    assertEquals(48, Contract.CURSOR_OPTIONS.byteSize());
  }

  @Test
  void aMismatchNamesTheFirstDifferingLine() {
    var error = assertThrows(IllegalStateException.class, () -> Contract.verify(name -> "size:1"));
    assertTrue(error.getMessage().contains("line 1"), error.getMessage());
    var schema =
        assertThrows(
            IllegalStateException.class,
            () -> Schema.verify(kind -> Native.schema(kind).replace("PageNumber", "Page")));
    assertTrue(schema.getMessage().contains("int:PageNumber"), schema.getMessage());
  }

  @Test
  void statusCodesMapToExceptions() {
    assertEquals(13, PdfException.of(13, "x", null).code());
    assertTrue(PdfException.of(13, "x", null) instanceof PdfException.Unsupported);
    assertTrue(PdfException.of(12, "x", null) instanceof PdfException.NativeError);
  }
}
