plugins {
    kotlin("jvm") version "2.2.20"
    `java-library`
    id("com.vanniktech.maven.publish") version "0.36.0"
}

group = "sa.ingenious"
version = "1.0.0"  // bumped by scripts/bump-version.sh

repositories {
    mavenCentral()
}

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(21))
    }
}

kotlin {
    jvmToolchain(21)
    sourceSets {
        val main by getting {
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
    from(nativeLibsDir) {
        into("natives")
    }
}

dependencies {
    implementation("net.java.dev.jna:jna:5.18.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-jdk8:1.10.2")

    testImplementation(kotlin("test"))
    testImplementation("org.junit.jupiter:junit-jupiter-api:5.13.4")
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine:5.13.4")
}

tasks.test {
    useJUnitPlatform()
}

mavenPublishing {
    publishToMavenCentral()
    signAllPublications()

    coordinates("sa.ingenious", "bolivar-uniffi-jvm", version.toString())

    pom {
        name.set("Bolivar UniFFI JVM Bindings")
        description.set("JVM bindings for the Bolivar PDF library via UniFFI")
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
