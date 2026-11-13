pluginManagement {
    repositories {
        // Central first: it serves the publish plugin from a CDN; the portal is a fallback.
        mavenCentral()
        gradlePluginPortal()
    }
}

plugins {
    // Lets Gradle download the Java 25 toolchain where none is installed.
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

rootProject.name = "bolivar"
