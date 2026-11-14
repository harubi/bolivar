package sa.ingenious.pdf.internal;

import java.io.IOException;
import java.io.InputStream;
import java.io.UncheckedIOException;
import java.lang.System.Logger.Level;
import java.lang.foreign.Arena;
import java.lang.foreign.SymbolLookup;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.FileAlreadyExistsException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.attribute.PosixFilePermissions;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Locale;
import java.util.Properties;
import java.util.regex.Pattern;
import org.jspecify.annotations.Nullable;

/**
 * Finds the native library and loads it once. Not part of the public API.
 *
 * <ul>
 *   <li>{@code -Dbolivar.library.path=/abs/libbolivar.so} loads that file.
 *   <li>Otherwise the library for this platform is extracted from the jar to {@code
 *       ${bolivar.cache.dir:-${java.io.tmpdir}}/bolivar-<user>/<version>-<sha256>/}, checked
 *       against its SHA-256, and reused by later runs.
 * </ul>
 *
 * <p>The build writes each library's SHA-256 beside it ({@code <library>.sha256}), so a later run
 * hashes only the cached copy and never reads the library out of the jar.
 */
@SuppressWarnings("restricted") // Restricted FFM calls are confined to this package.
public final class NativeLoader {
  private NativeLoader() {}

  public static final String LIBRARY_PATH_PROPERTY = "bolivar.library.path";
  public static final String CACHE_DIR_PROPERTY = "bolivar.cache.dir";
  public static final String LIB_NAME = "bolivar";
  private static final String VERSION_RESOURCE = "/bolivar-version.properties";
  private static final System.Logger LOG = System.getLogger("sa.ingenious.bolivar");
  private static final Pattern SHA256_HEX = Pattern.compile("[0-9a-f]{64}");

  private static volatile @Nullable SymbolLookup lookup;

  /**
   * The loaded library, located and loaded once. A failure is not remembered: the next call tries
   * again and reports its own cause.
   */
  static SymbolLookup lookup() {
    SymbolLookup loaded = lookup;
    if (loaded != null) {
      return loaded;
    }
    synchronized (NativeLoader.class) {
      loaded = lookup;
      if (loaded == null) {
        Path path = locate();
        loaded = SymbolLookup.libraryLookup(path, Arena.global());
        LOG.log(Level.DEBUG, "Loaded the Bolivar native library from {0}", path);
        lookup = loaded;
      }
      return loaded;
    }
  }

  /** The library to load: the override, or the extracted copy for this platform. */
  public static Path locate() {
    String override = System.getProperty(LIBRARY_PATH_PROPERTY);
    if (override != null && !override.isBlank()) {
      Path path = Path.of(override).toAbsolutePath();
      if (!Files.isRegularFile(path)) {
        throw new IllegalStateException(
            LIBRARY_PATH_PROPERTY + " points to a missing file: " + path);
      }
      return path;
    }
    String classifier = currentClassifier();
    String resource = resourcePath(classifier);
    try {
      return extract(resource, resource.substring(resource.lastIndexOf('/') + 1));
    } catch (IOException error) {
      throw new UncheckedIOException("Cannot extract the Bolivar native library", error);
    }
  }

  /** The jar resource for a classifier, e.g. {@code /natives/linux-x86-64/libbolivar.so}. */
  public static String resourcePath(String classifier) {
    return "/natives/" + classifier + "/" + System.mapLibraryName(LIB_NAME);
  }

  private static byte[] read(String resource) throws IOException {
    try (InputStream stream = NativeLoader.class.getResourceAsStream(resource)) {
      if (stream == null) {
        throw new IllegalStateException(
            "This jar has no Bolivar native library for "
                + currentClassifier()
                + " (looked for "
                + resource
                + "). Set -D"
                + LIBRARY_PATH_PROPERTY
                + " to a library you built.");
      }
      return stream.readAllBytes();
    }
  }

  /** The SHA-256 the build recorded for a resource, or null when it recorded none. */
  private static @Nullable String recordedHash(String resource) throws IOException {
    try (InputStream stream = NativeLoader.class.getResourceAsStream(resource + ".sha256")) {
      if (stream == null) {
        return null;
      }
      return parseRecordedHash(
          new String(stream.readAllBytes(), StandardCharsets.US_ASCII), resource + ".sha256");
    }
  }

  /**
   * The hash in a recorded {@code .sha256} file: the hash alone, or sha256sum's {@code <hash>
   * <file>}; null when the file is blank.
   *
   * @throws IllegalStateException when the first token is not 64 hex digits
   */
  static @Nullable String parseRecordedHash(String text, String what) {
    String trimmed = text.strip();
    if (trimmed.isEmpty()) {
      return null;
    }
    String hash = trimmed.split("\\s+", 2)[0].toLowerCase(Locale.ROOT);
    if (!SHA256_HEX.matcher(hash).matches()) {
      throw new IllegalStateException(
          "The Bolivar native library's recorded SHA-256 (" + what + ") is malformed");
    }
    return hash;
  }

  private static Path extract(String resource, String fileName) throws IOException {
    String recorded = recordedHash(resource);
    if (recorded != null) {
      Path target = target(recorded, fileName);
      if (matches(target, recorded)) {
        return target;
      }
    }
    byte[] bytes = read(resource);
    String hash = sha256(bytes);
    if (recorded != null && !recorded.equals(hash)) {
      throw new IllegalStateException(
          "The Bolivar native library in this jar does not match its recorded SHA-256");
    }
    return extract(bytes, hash, fileName);
  }

  /** Extract {@code bytes} to the cache; for tests and for jars without recorded hashes. */
  static Path extract(byte[] bytes, String fileName) throws IOException {
    return extract(bytes, sha256(bytes), fileName);
  }

  private static Path target(String hash, String fileName) throws IOException {
    Path dir = cacheRoot().resolve(jarVersion() + "-" + hash.substring(0, 16));
    createPrivateDirectories(dir);
    return dir.resolve(fileName);
  }

  private static boolean matches(Path target, String hash) throws IOException {
    return Files.isRegularFile(target) && sha256(Files.readAllBytes(target)).equals(hash);
  }

  private static Path extract(byte[] bytes, String hash, String fileName) throws IOException {
    Path target = target(hash, fileName);
    if (matches(target, hash)) {
      return target;
    }
    LOG.log(Level.DEBUG, "Extracting the Bolivar native library to {0}", target);
    Path temp = Files.createTempFile(target.getParent(), fileName, ".tmp");
    try {
      Files.write(temp, bytes);
      try {
        Files.move(temp, target, StandardCopyOption.ATOMIC_MOVE);
      } catch (AtomicMoveNotSupportedException | FileAlreadyExistsException error) {
        Files.move(temp, target, StandardCopyOption.REPLACE_EXISTING);
      }
      // What landed may not be what was written: a racing process or a full disk. Load nothing
      // unverified.
      if (!matches(target, hash)) {
        throw new IllegalStateException(
            "The extracted Bolivar native library at " + target + " does not match its SHA-256");
      }
    } catch (IOException error) {
      // Another process may have won the race, or holds the file open (Windows).
      if (!matches(target, hash)) {
        throw error;
      }
    } finally {
      Files.deleteIfExists(temp);
    }
    return target;
  }

  private static Path cacheRoot() {
    String configured = System.getProperty(CACHE_DIR_PROPERTY);
    Path base =
        configured != null && !configured.isBlank()
            ? Path.of(configured)
            : Path.of(System.getProperty("java.io.tmpdir"));
    String user = System.getProperty("user.name", "user").replaceAll("[^A-Za-z0-9._-]", "_");
    return base.resolve("bolivar-" + user);
  }

  private static void createPrivateDirectories(Path dir) throws IOException {
    Files.createDirectories(dir);
    try {
      var owner = PosixFilePermissions.fromString("rwx------");
      Path parent = dir.getParent();
      if (parent != null) {
        Files.setPosixFilePermissions(parent, owner);
      }
      Files.setPosixFilePermissions(dir, owner);
    } catch (UnsupportedOperationException error) {
      // Not a POSIX file system (Windows): the per-user temp directory already applies.
    }
  }

  static String sha256(byte[] bytes) {
    try {
      return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
    } catch (NoSuchAlgorithmException error) {
      throw new IllegalStateException(error);
    }
  }

  /** The jar's version, from {@code bolivar-version.properties}. */
  public static String jarVersion() {
    var properties = new Properties();
    try (InputStream stream = NativeLoader.class.getResourceAsStream(VERSION_RESOURCE)) {
      if (stream == null) {
        throw new IllegalStateException("Missing Bolivar version resource " + VERSION_RESOURCE);
      }
      properties.load(stream);
    } catch (IOException error) {
      throw new UncheckedIOException(error);
    }
    String version = properties.getProperty("version");
    if (version == null || version.isBlank()) {
      throw new IllegalStateException("Missing Bolivar JVM version");
    }
    return version;
  }

  /** This platform's classifier, e.g. {@code linux-x86-64}. */
  public static String currentClassifier() {
    return classifierFor(
        System.getProperty("os.name", ""), System.getProperty("os.arch", ""), isMusl());
  }

  /** The classifier for an OS and architecture, as release builds name their directories. */
  public static String classifierFor(String osName, String archName, boolean musl) {
    String os = osName.toLowerCase(Locale.ROOT);
    String normalizedOs;
    if (os.contains("mac") || os.contains("darwin")) {
      normalizedOs = "macos";
    } else if (os.contains("win")) {
      normalizedOs = "windows";
    } else if (os.contains("linux")) {
      if (musl) {
        throw new IllegalStateException(
            "Bolivar ships no native library for musl libc (e.g. Alpine). Use a glibc-based"
                + " image, or build bolivar-ffi and set -D"
                + LIBRARY_PATH_PROPERTY
                + ".");
      }
      normalizedOs = "linux";
    } else {
      throw new IllegalStateException("Unsupported os.name: " + osName);
    }
    String normalizedArch =
        switch (archName.toLowerCase(Locale.ROOT)) {
          case "x86_64", "amd64" -> "x86-64";
          case "aarch64", "arm64" -> "aarch64";
          default -> throw new IllegalStateException("Unsupported os.arch: " + archName);
        };
    return normalizedOs + "-" + normalizedArch;
  }

  /** Whether this Linux uses musl libc: its dynamic loader names it. */
  static boolean isMusl() {
    if (!System.getProperty("os.name", "").toLowerCase(Locale.ROOT).contains("linux")) {
      return false;
    }
    try (var entries = Files.list(Path.of("/lib"))) {
      return entries.anyMatch(path -> path.getFileName().toString().startsWith("ld-musl-"));
    } catch (IOException | SecurityException error) {
      return false;
    }
  }
}
