rootProject.name = "just_speech_core"

// The server pulls this build in with includeBuild("core") and depends on
// kr.justspeech.core:<project name>.
include(":qa-prompt")
project(":qa-prompt").projectDir = file("qa-prompt/kotlin")
