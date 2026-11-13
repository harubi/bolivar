plugins {
    `java-library`
    id("com.vanniktech.maven.publish") version "0.37.0"
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
        languageVersion.set(JavaLanguageVersion.of(25))
    }
}

sourceSets {
    named("main") {
        resources.srcDir("src/main/clojure")
    }
    named("test") {
        resources.srcDir("src/test/clojure")
    }
}

tasks.withType<JavaCompile>().configureEach {
    options.release.set(25)
    options.compilerArgs.addAll(listOf("-Xlint:all", "-Werror"))
}

// Javadoc fails on any problem except undocumented record components, which name themselves.
tasks.withType<Javadoc>().configureEach {
    (options as StandardJavadocDocletOptions).apply {
        addBooleanOption("Xdoclint:all,-missing", true)
        addBooleanOption("Werror", true)
    }
}

// Native libraries, one directory per classifier (e.g. natives/linux-x86-64/).
val nativeLibsDir = layout.buildDirectory.dir("native-libs")

tasks.register<Copy>("copyNativeLibs") {
    from("natives")
    into(nativeLibsDir)
}

tasks.named<Jar>("jar") {
    dependsOn("copyNativeLibs")
    manifest {
        attributes(
            "Implementation-Version" to project.version,
            // Only `java -jar` reads this; library users pass the flag.
            "Enable-Native-Access" to "ALL-UNNAMED",
        )
    }
    from(nativeLibsDir) {
        into("natives")
    }
}

// Read at configuration time: Task.project at execution time fails in Gradle 10.
val bolivarVersion = version.toString()

tasks.processResources {
    inputs.property("bolivarVersion", bolivarVersion)
    filesMatching("bolivar-version.properties") {
        expand("version" to bolivarVersion)
    }
}

// Tests load the native library from natives/ through the real loader.
val testNatives = layout.projectDirectory.dir("natives")

dependencies {
    testImplementation("org.clojure:clojure:1.12.6")
    testImplementation(platform("org.junit:junit-bom:6.1.3"))
    testImplementation("org.junit.jupiter:junit-jupiter-api")
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()
    jvmArgs("--enable-native-access=ALL-UNNAMED")
    systemProperty("bolivar.test.repo", rootDir.resolve("../../..").canonicalPath)
    classpath += files(layout.buildDirectory.dir("native-libs-root"))
}

// Expose natives/ on the test classpath under /natives/ like the jar does.
val testNativeResources = tasks.register<Copy>("testNativeResources") {
    from(testNatives)
    into(layout.buildDirectory.dir("native-libs-root/natives"))
}
tasks.test { dependsOn(testNativeResources) }

tasks.register<JavaExec>("clojureTest") {
    group = LifecycleBasePlugin.VERIFICATION_GROUP
    description = "Run Clojure tests"
    dependsOn("testClasses", testNativeResources)
    classpath = sourceSets["test"].runtimeClasspath + files(layout.buildDirectory.dir("native-libs-root"))
    mainClass.set("clojure.main")
    jvmArgs("--enable-native-access=ALL-UNNAMED")
    systemProperty("bolivar.test.repo", rootDir.resolve("../../..").canonicalPath)
    args("-m", "sa.ingenious.test-runner")
}

tasks.named("check") {
    dependsOn("clojureTest")
}

mavenPublishing {
    publishToMavenCentral()
    if (providers.gradleProperty("local.skipSigning").orNull != "true") {
        signAllPublications()
    }

    coordinates("sa.ingenious", "bolivar", version.toString())

    pom {
        name.set("Bolivar JVM Bindings")
        description.set("JVM bindings for the Bolivar PDF library")
        inceptionYear.set("2025")
        url.set("https://github.com/harubi/bolivar")

        licenses {
            license {
                name.set("MIT License")
                url.set("https://opensource.org/licenses/MIT")
            }
        }
        developers {
            developer {
                id.set("harubi")
                name.set("harubi")
                url.set("https://github.com/harubi")
            }
        }
        scm {
            url.set("https://github.com/harubi/bolivar")
            connection.set("scm:git:git://github.com/harubi/bolivar.git")
            developerConnection.set("scm:git:ssh://git@github.com/harubi/bolivar.git")
        }
    }
}
