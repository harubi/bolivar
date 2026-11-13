import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    kotlin("jvm") version "2.4.21"
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
        languageVersion.set(JavaLanguageVersion.of(21))
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

kotlin {
    jvmToolchain(21)
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_21)
    }
    sourceSets {
        named("main") {
            kotlin.srcDir("../kotlin")
        }
    }
}

// Native library bundling into JAR
val nativeLibsDir = layout.buildDirectory.dir("native-libs")

tasks.register<Copy>("copyNativeLibs") {
    from("natives")
    into(nativeLibsDir)
}

tasks.named<Jar>("jar") {
    dependsOn("copyNativeLibs")
    manifest {
        attributes["Implementation-Version"] = project.version
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

dependencies {
    implementation("net.java.dev.jna:jna:5.19.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-jdk8:1.11.0")

    testImplementation("org.clojure:clojure:1.12.6")
    testImplementation(kotlin("test"))
    testImplementation(platform("org.junit:junit-bom:6.1.3"))
    testImplementation("org.junit.jupiter:junit-jupiter-api")
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()
}

tasks.register<JavaExec>("clojureTest") {
    group = LifecycleBasePlugin.VERIFICATION_GROUP
    description = "Run Clojure wrapper tests"
    dependsOn("testClasses")
    classpath = sourceSets["test"].runtimeClasspath
    mainClass.set("clojure.main")
    args("-m", "sa.ingenious.pdf-test")
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
