package kr.justspeech.core.qaprompt

/**
 * The Q&A system prompt: the presentation content (title, full script) plus the answer rules
 * (language, tone, optional line cap). The app builds the same text (`qa-prompt/dart`) for a
 * device-only presentation's direct connection; the two must not answer differently, so both are
 * pinned to `qa-prompt/vectors`.
 */
data class QaAnswerPrompt(
	val title: String?,
	val language: String,
	val qaAnswerTone: String,
	val qaAnswerMaxLineCount: Int?,
	val scriptText: String,
) {
	fun systemPrompt(): String = buildString {
		appendLine("You write answer scripts that a presenter reads aloud verbatim to answer audience questions during a live presentation.")
		appendLine("Rules:")
		appendLine("- Answer in this language: ${inline(language)}.")
		toneRule(qaAnswerTone)?.let { appendLine(it) }
		qaAnswerMaxLineCount?.let { appendLine("- Use at most $it lines.") }
		// ~200 characters is 20–30 seconds read aloud: enough to answer, short
		// enough not to stall the live Q&A.
		appendLine("- Keep the answer to about 200 characters.")
		appendLine("- Output only the answer script itself — no preamble, no meta commentary.")
		// The answer is shown and read aloud as plain text: markup would surface literally.
		appendLine("- Plain text only: no markdown or emphasis markup such as **example**.")
		appendLine("- Ground the answer in the presentation script below; if it does not cover the question, answer briefly from general knowledge without presenting guesses as facts.")
		appendLine()
		appendLine("Presentation title: ${title?.let(::inline) ?: "(untitled)"}")
		appendLine("Presentation script:")
		append(scriptText)
	}

	/** AUTO adds no rule: the model matches the presentation. Any other value is inlined as typed. */
	private fun toneRule(tone: String): String? =
		when (collapse(tone).uppercase()) {
			"AUTO" -> null
			"NATURAL" -> "- Tone: natural and conversational."
			"FORMAL" -> "- Tone: formal and polished."
			else -> "- Tone: ${inline(tone)}."
		}

	/** Caller-supplied free strings inlined into single prompt lines. The script is exempt: it is free-form grounding content. */
	private fun inline(value: String): String = collapse(value).take(MAX_INLINE_LENGTH)

	private fun collapse(value: String): String =
		value.replace(WHITESPACE, " ").removePrefix(" ").removeSuffix(" ")

	private companion object {
		/** UTF-16 units, as both languages count them. */
		const val MAX_INLINE_LENGTH = 100

		/**
		 * Unicode White_Space plus the BOM, spelled out: Java's `\s` is ASCII-only, and a U+2028 left
		 * in a line would let a crafted value start a rule line of its own.
		 */
		val WHITESPACE = Regex("[\\t\\n\\u000B\\f\\r \\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000\\uFEFF]+")
	}
}
