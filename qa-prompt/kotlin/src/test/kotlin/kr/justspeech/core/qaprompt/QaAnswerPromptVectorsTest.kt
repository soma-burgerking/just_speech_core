package kr.justspeech.core.qaprompt

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class QaAnswerPromptVectorsTest {
	@Test
	fun `every shared case builds the same prompt`() {
		val cases = Json.parseToJsonElement(File(System.getProperty("qaPromptVectors")).readText()).jsonArray
		assertTrue(cases.isNotEmpty())
		for (case in cases.map { it.jsonObject }) {
			fun string(key: String) = case.getValue(key).jsonPrimitive.content
			val prompt = QaAnswerPrompt(
				title = case.getValue("title").takeUnless { it is JsonNull }?.jsonPrimitive?.content,
				language = string("language"),
				qaAnswerTone = string("tone"),
				qaAnswerMaxLineCount = case.getValue("maxLines").takeUnless { it is JsonNull }?.jsonPrimitive?.int,
				scriptText = string("script"),
			).systemPrompt()
			assertEquals(string("prompt"), prompt, string("name"))
		}
	}
}
