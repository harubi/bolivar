plugins {
    kotlin("jvm") version "2.2.20"
    `java-library`
}

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
