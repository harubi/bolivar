package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Without {@code --enable-native-access} the JVM warns and Bolivar still works (Java 25). If a
 * future JDK denies the call instead, this test fails, and the README must change with it.
 */
class NativeAccessTest {
  /** Entry point of the child JVM. */
  public static void main(String[] args) {
    System.out.println(Document.extractText(args[0]).contains("Hello") ? "ok" : "bad");
  }

  @Test
  void withoutTheFlagTheJvmOnlyWarns() throws IOException, InterruptedException {
    String java = Path.of(System.getProperty("java.home"), "bin", "java").toString();
    var command =
        List.of(
            java,
            "-cp",
            System.getProperty("java.class.path"),
            "-Dbolivar.cache.dir=" + System.getProperty("java.io.tmpdir"),
            NativeAccessTest.class.getName(),
            Fixtures.path("simple1.pdf"));
    Process process = new ProcessBuilder(command).start();
    String out = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
    String err = new String(process.getErrorStream().readAllBytes(), StandardCharsets.UTF_8);
    assertEquals(0, process.waitFor(), err);
    assertEquals("ok", out.strip());
    assertTrue(err.contains("restricted method"), err);
    assertTrue(err.contains("--enable-native-access=ALL-UNNAMED"), err);
  }

  /**
   * On a runtime without {@code jdk.jfr} (a minimal jlink image), Bolivar still loads and works.
   */
  @Test
  void worksWithoutTheFlightRecorderModule() throws IOException, InterruptedException {
    String java = Path.of(System.getProperty("java.home"), "bin", "java").toString();
    var command =
        List.of(
            java,
            "--limit-modules",
            "java.base",
            "--enable-native-access=ALL-UNNAMED",
            "-cp",
            System.getProperty("java.class.path"),
            "-Dbolivar.cache.dir=" + System.getProperty("java.io.tmpdir"),
            NativeAccessTest.class.getName(),
            Fixtures.path("simple1.pdf"));
    Process process = new ProcessBuilder(command).start();
    String out = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
    String err = new String(process.getErrorStream().readAllBytes(), StandardCharsets.UTF_8);
    assertEquals(0, process.waitFor(), err);
    assertEquals("ok", out.strip(), err);
  }
}
