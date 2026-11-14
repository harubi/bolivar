package sa.ingenious.pdf.internal;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class NativeLoaderTest {
  @Test
  void classifiersMatchReleaseDirectories() {
    assertEquals("linux-x86-64", NativeLoader.classifierFor("Linux", "amd64", false));
    assertEquals("linux-aarch64", NativeLoader.classifierFor("Linux", "aarch64", false));
    assertEquals("macos-aarch64", NativeLoader.classifierFor("Mac OS X", "aarch64", false));
    assertEquals("macos-x86-64", NativeLoader.classifierFor("Mac OS X", "x86_64", false));
    assertEquals("windows-x86-64", NativeLoader.classifierFor("Windows 11", "amd64", false));
  }

  @Test
  void muslAndUnknownPlatformsFailClearly() {
    var musl =
        assertThrows(
            IllegalStateException.class, () -> NativeLoader.classifierFor("Linux", "amd64", true));
    assertTrue(musl.getMessage().contains("musl"));
    assertThrows(
        IllegalStateException.class, () -> NativeLoader.classifierFor("Plan 9", "amd64", false));
    assertThrows(
        IllegalStateException.class, () -> NativeLoader.classifierFor("Linux", "riscv64", false));
  }

  @Test
  void resourcePathsUseThePlatformLibraryName() {
    assertEquals(
        "/natives/linux-x86-64/" + System.mapLibraryName("bolivar"),
        NativeLoader.resourcePath("linux-x86-64"));
  }

  @Test
  void extractionIsReusedAndHashChecked(@TempDir Path cache) throws Exception {
    String previous = System.getProperty(NativeLoader.CACHE_DIR_PROPERTY);
    System.setProperty(NativeLoader.CACHE_DIR_PROPERTY, cache.toString());
    try {
      byte[] bytes = "not really a library".getBytes(StandardCharsets.UTF_8);
      Path first = NativeLoader.extract(bytes, "libx.so");
      long modified = Files.getLastModifiedTime(first).toMillis();
      Path second = NativeLoader.extract(bytes, "libx.so");
      assertEquals(first, second);
      assertEquals(modified, Files.getLastModifiedTime(second).toMillis());
      // A corrupted cached copy is replaced.
      Files.write(first, "tampered".getBytes(StandardCharsets.UTF_8));
      Path third = NativeLoader.extract(bytes, "libx.so");
      assertEquals(NativeLoader.sha256(bytes), NativeLoader.sha256(Files.readAllBytes(third)));
      // Different bytes land in a different directory.
      Path other = NativeLoader.extract("other".getBytes(StandardCharsets.UTF_8), "libx.so");
      assertTrue(!other.getParent().equals(first.getParent()));
    } finally {
      if (previous == null) {
        System.clearProperty(NativeLoader.CACHE_DIR_PROPERTY);
      } else {
        System.setProperty(NativeLoader.CACHE_DIR_PROPERTY, previous);
      }
    }
  }

  @Test
  void aMissingOverrideFailsClearly() {
    String previous = System.getProperty(NativeLoader.LIBRARY_PATH_PROPERTY);
    System.setProperty(NativeLoader.LIBRARY_PATH_PROPERTY, "/nonexistent/libbolivar.so");
    try {
      var error = assertThrows(IllegalStateException.class, NativeLoader::locate);
      assertTrue(error.getMessage().contains(NativeLoader.LIBRARY_PATH_PROPERTY));
    } finally {
      if (previous == null) {
        System.clearProperty(NativeLoader.LIBRARY_PATH_PROPERTY);
      } else {
        System.setProperty(NativeLoader.LIBRARY_PATH_PROPERTY, previous);
      }
    }
  }

  @Test
  void recordedHashesParseInEitherFormatAndRejectGarbage() {
    String hex = "0123456789abcdef".repeat(4);
    assertEquals(hex, NativeLoader.parseRecordedHash(hex + "\n", "t"));
    assertEquals(hex, NativeLoader.parseRecordedHash(hex + "  libbolivar.dylib\n", "t"));
    assertEquals(hex, NativeLoader.parseRecordedHash(hex.toUpperCase(Locale.ROOT), "t"));
    assertNull(NativeLoader.parseRecordedHash("  \n", "t"));
    for (String bad : new String[] {"abc", hex.substring(1) + "z", hex + "0"}) {
      assertThrows(
          IllegalStateException.class, () -> NativeLoader.parseRecordedHash(bad, "t"), bad);
    }
  }
}
