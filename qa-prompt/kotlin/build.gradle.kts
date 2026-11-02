plugins {
	// The server's version: the plugin is loaded by both builds in one Gradle run.
	kotlin("jvm") version "2.3.21"
}

group = "kr.justspeech.core"

repositories {
	mavenCentral()
}

kotlin {
	jvmToolchain(21)
}

dependencies {
	testImplementation(kotlin("test"))
	testImplementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
	testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
	useJUnitPlatform()
	systemProperty("qaPromptVectors", file("../vectors/cases.json").absolutePath)
}
