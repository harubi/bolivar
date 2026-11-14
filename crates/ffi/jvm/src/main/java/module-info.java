/**
 * Bolivar: PDF text, layout and table extraction backed by a native Rust library.
 *
 * <p>Allow native access with {@code --enable-native-access=sa.ingenious.bolivar} on the module
 * path, or {@code --enable-native-access=ALL-UNNAMED} on the class path.
 */
module sa.ingenious.bolivar {
  requires static transitive org.jspecify;
  // Optional: without it (a minimal jlink image) Flight Recorder events are skipped.
  requires static jdk.jfr;

  exports sa.ingenious.pdf;
}
