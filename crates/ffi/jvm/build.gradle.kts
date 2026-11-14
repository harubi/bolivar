import java.security.MessageDigest
import net.ltgt.gradle.errorprone.CheckSeverity
import net.ltgt.gradle.errorprone.errorprone
import net.ltgt.gradle.nullaway.nullaway

buildscript {
    // Lock the plugin classpath too, not only the project's configurations.
    configurations.classpath {
        resolutionStrategy.activateDependencyLocking()
    }
}

plugins {
    `java-library`
    alias(libs.plugins.maven.publish)
    alias(libs.plugins.errorprone)
    alias(libs.plugins.nullaway)
    alias(libs.plugins.spotless)
    alias(libs.plugins.jmh)
}

val cargoWorkspaceVersion =
    file("../../../Cargo.toml")
        .readText()
        .substringAfter("[workspace.package]")
        .lineSequence()
        .first { it.startsWith("version = ") }
        .substringAfter('"')
        .substringBefore('"')

group = "sa.ingenious"
version = cargoWorkspaceVersion

repositories {
    mavenCentral()
}

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(25)
    }
}

dependencyLocking {
    lockAllConfigurations()
}

sourceSets {
    named("main") {
        resources.srcDir("src/main/clojure")
    }
    named("test") {
        resources.srcDir("src/test/clojure")
    }
}

dependencies {
    api(libs.jspecify)
    errorprone(libs.errorprone.core)
    errorprone(libs.nullaway)

    testImplementation(libs.clojure)
    testImplementation(libs.test.check)
    testImplementation(platform(libs.junit.bom))
    testImplementation(libs.junit.jupiter)
    testImplementation(libs.archunit.junit)
    testRuntimeOnly(libs.junit.launcher)

    jmh(libs.jmh.core)
    jmhAnnotationProcessor(libs.jmh.generator)
}

nullaway {
    onlyNullMarked = true
    jspecifyMode = true
    error()
}

tasks.withType<JavaCompile>().configureEach {
    options.release = 25
    options.compilerArgs.addAll(listOf("-Xlint:all", "-Werror"))
    options.errorprone {
        disableWarningsInGeneratedCode = true
        // Generated JMH harness code is not ours to fix.
        excludedPaths = ".*/build/generated/.*"
    }
}

// Tests and benchmarks are not null-marked; NullAway checks the library.
tasks.named<JavaCompile>("compileTestJava") {
    options.errorprone.nullaway.severity = CheckSeverity.OFF
}
tasks.named<JavaCompile>("compileJmhJava") {
    options.errorprone.nullaway.severity = CheckSeverity.OFF
    // The JMH generator emits code that trips -Xlint.
    options.compilerArgs.removeAll(listOf("-Werror"))
}

// Javadoc fails on any problem except undocumented members, which name themselves.
tasks.withType<Javadoc>().configureEach {
    (options as StandardJavadocDocletOptions).apply {
        addBooleanOption("Xdoclint:all,-missing", true)
        addBooleanOption("Werror", true)
    }
}

spotless {
    java {
        target("src/*/java/**/*.java")
        googleJavaFormat(libs.versions.googleJavaFormat.get())
    }
}

/** Copies native libraries and writes each one's SHA-256 beside it, for the loader. */
abstract class StageNatives : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val natives: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun stage() {
        val out = output.get().asFile
        out.deleteRecursively()
        natives.asFileTree.visit {
            if (!isDirectory) {
                val target = relativePath.getFile(out)
                target.parentFile.mkdirs()
                file.copyTo(target, overwrite = true)
                val digest = MessageDigest.getInstance("SHA-256").digest(file.readBytes())
                val hex = digest.joinToString("") { "%02x".format(it) }
                File(target.path + ".sha256").writeText(hex + "\n")
            }
        }
    }
}

// Native libraries, one directory per classifier (e.g. natives/linux-x86-64/).
val stageNatives =
    tasks.register<StageNatives>("stageNatives") {
        natives.from(layout.projectDirectory.dir("natives"))
        output = layout.buildDirectory.dir("native-libs/natives")
    }

tasks.named<Jar>("jar") {
    manifest {
        attributes(
            "Implementation-Version" to project.version,
            // Only `java -jar` reads this; library users pass the flag.
            "Enable-Native-Access" to "ALL-UNNAMED",
        )
    }
    from(stageNatives) {
        into("natives")
    }
}

val repoRoot = rootDir.resolve("../../..").canonicalPath

tasks.processResources {
    // A local copy: the configuration cache cannot serialise references to the script.
    val bolivarVersion = project.version.toString()
    inputs.property("bolivarVersion", bolivarVersion)
    filesMatching("bolivar-version.properties") {
        expand("version" to bolivarVersion)
    }
}

// Tests load the native library from the staged natives through the real loader.
val nativeRoot = files(layout.buildDirectory.dir("native-libs")).builtBy(stageNatives)

tasks.test {
    useJUnitPlatform {
        excludeTags("benchmark")
    }
    jvmArgs("--enable-native-access=ALL-UNNAMED")
    systemProperty("bolivar.test.repo", repoRoot)
    classpath += nativeRoot
}

tasks.register<Test>("benchmark") {
    group = LifecycleBasePlugin.VERIFICATION_GROUP
    description = "Check the binding's cost against its budget"
    testClassesDirs = sourceSets["test"].output.classesDirs
    classpath = sourceSets["test"].runtimeClasspath + nativeRoot
    useJUnitPlatform {
        includeTags("benchmark")
    }
    jvmArgs("--enable-native-access=ALL-UNNAMED")
    systemProperty("bolivar.test.repo", repoRoot)
    systemProperty("bolivar.bench.documents", providers.gradleProperty("benchDocuments").orElse("").get())
    testLogging {
        showStandardStreams = true
    }
    outputs.upToDateWhen { false }
}

tasks.register<JavaExec>("clojureTest") {
    group = LifecycleBasePlugin.VERIFICATION_GROUP
    description = "Run Clojure tests"
    classpath = sourceSets["test"].runtimeClasspath + nativeRoot
    mainClass = "clojure.main"
    jvmArgs("--enable-native-access=ALL-UNNAMED")
    systemProperty("bolivar.test.repo", repoRoot)
    args("-m", "sa.ingenious.test-runner")
}

jmh {
    jvmArgsAppend = listOf("--enable-native-access=ALL-UNNAMED", "-Dbolivar.test.repo=$repoRoot")
}

tasks.named<Jar>("jmhJar") { from(stageNatives) { into("natives") } }

tasks.named("check") {
    dependsOn("clojureTest", "jmhClasses")
}

// The version comes from Cargo.toml, so a local publish would overwrite a released version in
// ~/.m2 and break every project on the machine that depends on it. Releases publish to Central.
tasks.withType<PublishToMavenLocal>().configureEach {
    doFirst {
        throw GradleException(
            "Publishing Bolivar to Maven Local is not allowed: it would replace the released " +
                "artifacts of this version in ~/.m2. Depend on the built jar instead.",
        )
    }
}

mavenPublishing {
    publishToMavenCentral()
    if (providers.gradleProperty("local.skipSigning").orNull != "true") {
        signAllPublications()
    }

    coordinates("sa.ingenious", "bolivar", version.toString())

    pom {
        name = "Bolivar JVM Bindings"
        description = "JVM bindings for the Bolivar PDF library"
        inceptionYear = "2025"
        url = "https://github.com/harubi/bolivar"

        licenses {
            license {
                name = "MIT License"
                url = "https://opensource.org/licenses/MIT"
            }
        }
        developers {
            developer {
                id = "harubi"
                name = "harubi"
                url = "https://github.com/harubi"
            }
        }
        scm {
            url = "https://github.com/harubi/bolivar"
            connection = "scm:git:git://github.com/harubi/bolivar.git"
            developerConnection = "scm:git:ssh://git@github.com/harubi/bolivar.git"
        }
    }
}
