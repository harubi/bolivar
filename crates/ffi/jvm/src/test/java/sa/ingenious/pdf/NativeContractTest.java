package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;
import sa.ingenious.pdf.internal.Native;
import sa.ingenious.pdf.internal.NativeLoader;
import sa.ingenious.pdf.internal.Schema;

/** The jar and the native library agree on versions, layouts and column schemas. */
class NativeContractTest {
  @Test
  void versionsMatch() {
    Native.ensureLoaded();
    assertEquals(Native.ABI_VERSION, Native.abiVersion());
    assertEquals(NativeLoader.jarVersion(), Document.version());
  }

  @Test
  void structLayoutsMatchNativeSizes() {
    Native.ensureLoaded();
    long[] expected = {
      NativeStructs.OPTIONS.byteSize(),
      NativeStructs.TABLE_OPTIONS.byteSize(),
      Native.ERROR_OUT.byteSize(),
      Native.BATCH_HEADER.byteSize(),
      Native.COLUMN_DESC.byteSize(),
    };
    assertArrayEquals(expected, Native.structSizes());
  }

  @Test
  void everySchemaMatchesNative() {
    Native.ensureLoaded();
    for (int kind : Schema.KINDS) {
      assertEquals(Schema.text(kind), Native.schema(kind), "kind " + kind);
    }
  }
}
