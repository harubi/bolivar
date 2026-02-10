package sa.ingenious.bolivar

import java.nio.file.Files
import java.nio.file.StandardCopyOption

object BolivarNativeLoader {
    const val COMPONENT_NAME: String = "bolivar"
    const val LIB_NAME: String = "bolivar_uniffi"
    const val LIB_OVERRIDE_PROPERTY: String = "uniffi.component.$COMPONENT_NAME.libraryOverride"

    @Volatile
    private var loadedPath: String? = null

    @JvmStatic
    fun configureLibraryOverride(pathOrName: String) {
        System.setProperty(LIB_OVERRIDE_PROPERTY, pathOrName)
    }

    @JvmStatic
    fun currentClassifier(): String {
        val os = normalizedOs()
        val arch = normalizedArch()
        return "$os-$arch"
    }

    @JvmStatic
    fun defaultResourcePath(): String {
        val libFileName = System.mapLibraryName(LIB_NAME)
        return "/natives/${currentClassifier()}/$libFileName"
    }

    @JvmStatic
    fun loadFromClasspath(resourcePath: String = defaultResourcePath()): String {
        loadedPath?.let { return it }
        synchronized(this) {
            loadedPath?.let { return it }

            val override = System.getProperty(LIB_OVERRIDE_PROPERTY)
            if (!override.isNullOrBlank()) {
                loadedPath = override
                return override
            }

            val stream = BolivarNativeLoader::class.java.getResourceAsStream(resourcePath)
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
                loadedPath = absolutePath
                return absolutePath
            }
        }
    }

    private fun fileNameParts(fileName: String): Pair<String, String> {
        val dot = fileName.lastIndexOf('.')
        val rawPrefix = if (dot > 0) fileName.substring(0, dot) else fileName
        val prefix = rawPrefix.take(48).padEnd(3, '_')
        val suffix = if (dot > 0) fileName.substring(dot) else ".tmp"
        return prefix to suffix
    }

    private fun normalizedOs(): String {
        val osName = System.getProperty("os.name").lowercase()
        return when {
            osName.contains("mac") || osName.contains("darwin") -> "macos"
            osName.contains("win") -> "windows"
            osName.contains("linux") -> "linux"
            else -> throw IllegalStateException("Unsupported os.name: ${System.getProperty("os.name")}")
        }
    }

    private fun normalizedArch(): String {
        val arch = System.getProperty("os.arch").lowercase()
        return when (arch) {
            "x86_64", "amd64" -> "x86_64"
            "aarch64", "arm64" -> "aarch64"
            else -> throw IllegalStateException("Unsupported os.arch: ${System.getProperty("os.arch")}")
        }
    }
}
