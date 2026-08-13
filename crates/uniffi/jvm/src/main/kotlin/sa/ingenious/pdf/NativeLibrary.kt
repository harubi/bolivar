package sa.ingenious.pdf

import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.Properties
import sa.ingenious.ffi.bolivarVersion
import sa.ingenious.ffi.uniffiEnsureInitialized

object NativeLibrary {
    const val COMPONENT_NAME: String = "bolivar"
    const val LIB_NAME: String = "bolivar_uniffi"
    const val LIB_OVERRIDE_PROPERTY: String = "uniffi.component.$COMPONENT_NAME.libraryOverride"
    private const val VERSION_RESOURCE: String = "/bolivar-version.properties"

    @Volatile
    private var loadedPath: String? = null

    @JvmStatic
    fun configureLibraryOverride(pathOrName: String) {
        System.setProperty(LIB_OVERRIDE_PROPERTY, pathOrName)
    }

    @JvmStatic
    fun currentClassifier(): String {
        val os = normalizedOs(System.getProperty("os.name"))
        val arch = normalizedArch(System.getProperty("os.arch"))
        return "$os-$arch"
    }

    internal fun classifierFor(
        osName: String,
        archName: String,
    ): String = "${normalizedOs(osName)}-${normalizedArch(archName)}"

    @JvmStatic
    fun defaultResourcePath(): String {
        val libFileName = System.mapLibraryName(LIB_NAME)
        return "/natives/${currentClassifier()}/$libFileName"
    }

    @JvmStatic
    @JvmOverloads
    fun load(resourcePath: String = defaultResourcePath()): String {
        loadedPath?.let { return it }
        synchronized(this) {
            loadedPath?.let { return it }

            val override = System.getProperty(LIB_OVERRIDE_PROPERTY)
            if (!override.isNullOrBlank()) {
                verifyNativeVersion()
                loadedPath = override
                return override
            }

            val stream =
                NativeLibrary::class.java.getResourceAsStream(resourcePath)
                    ?: throw IllegalStateException("Missing native resource at $resourcePath")

            stream.use {
                val fileName = resourcePath.substringAfterLast('/')
                val (prefix, suffix) = fileNameParts(fileName)
                val temp = Files.createTempFile(prefix, suffix)
                temp.toFile().deleteOnExit()
                Files.copy(it, temp, StandardCopyOption.REPLACE_EXISTING)

                val absolutePath = temp.toAbsolutePath().toString()
                System.load(absolutePath)
                configureLibraryOverride(absolutePath)
                verifyNativeVersion()
                loadedPath = absolutePath
                return absolutePath
            }
        }
    }

    internal fun loadFromClasspath(): String = load()

    internal fun requireMatchingVersion(
        expected: String,
        actual: String,
    ) {
        check(actual == expected) {
            "Bolivar version mismatch: JVM $expected, native $actual"
        }
    }

    private fun verifyNativeVersion() {
        uniffiEnsureInitialized()
        requireMatchingVersion(expectedVersion(), bolivarVersion())
    }

    private fun expectedVersion(): String {
        val properties = Properties()
        val stream =
            NativeLibrary::class.java.getResourceAsStream(VERSION_RESOURCE)
                ?: error("Missing Bolivar version resource $VERSION_RESOURCE")
        stream.use(properties::load)
        return properties.getProperty("version")
            ?.takeIf(String::isNotBlank)
            ?: error("Missing Bolivar JVM version")
    }

    private fun fileNameParts(fileName: String): Pair<String, String> {
        val dot = fileName.lastIndexOf('.')
        val rawPrefix = if (dot > 0) fileName.substring(0, dot) else fileName
        val prefix = rawPrefix.take(48).padEnd(3, '_')
        val suffix = if (dot > 0) fileName.substring(dot) else ".tmp"
        return prefix to suffix
    }

    private fun normalizedOs(osName: String): String {
        val value = osName.lowercase()
        return when {
            value.contains("mac") || value.contains("darwin") -> "macos"
            value.contains("win") -> "windows"
            value.contains("linux") -> "linux"
            else -> throw IllegalStateException("Unsupported os.name: $osName")
        }
    }

    private fun normalizedArch(archName: String): String {
        return when (archName.lowercase()) {
            "x86_64", "amd64" -> "x86-64"
            "aarch64", "arm64" -> "aarch64"
            else -> throw IllegalStateException("Unsupported os.arch: $archName")
        }
    }
}
