package sa.ingenious.pdf;

import java.nio.file.Path;

final class Fixtures {
  private Fixtures() {}

  static final Path REPO = Path.of(System.getProperty("bolivar.test.repo", "../../.."));

  static Path fixture(String name) {
    return REPO.resolve("crates/core/tests/fixtures").resolve(name);
  }

  static String path(String name) {
    return fixture(name).toString();
  }
}
