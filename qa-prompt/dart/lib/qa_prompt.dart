/// The Q&A system prompt. The server builds the same text (`qa-prompt/kotlin`)
/// for the same content, and a device-only presentation's direct connection
/// sends this one: the two must not answer differently, so both are pinned to
/// `qa-prompt/vectors`.
String qaSystemPrompt({
  required String? title,
  required String language,
  required String tone,
  required int? maxLines,
  required String script,
}) {
  final buffer = StringBuffer()
    ..writeln(
      'You write answer scripts that a presenter reads aloud verbatim to answer audience questions during a live presentation.',
    )
    ..writeln('Rules:')
    ..writeln('- Answer in this language: ${_inline(language)}.');
  final toneRule = _toneRule(tone);
  if (toneRule != null) buffer.writeln(toneRule);
  if (maxLines != null) buffer.writeln('- Use at most $maxLines lines.');
  buffer
    // ~200 characters is 20–30 seconds read aloud: enough to answer, short
    // enough not to stall the live Q&A.
    ..writeln('- Keep the answer to about 200 characters.')
    ..writeln(
      '- Output only the answer script itself — no preamble, no meta commentary.',
    )
    // The answer is shown and read aloud as plain text: markup would surface
    // literally.
    ..writeln(
      '- Plain text only: no markdown or emphasis markup such as **example**.',
    )
    ..writeln(
      '- Ground the answer in the presentation script below; if it does not cover the question, answer briefly from general knowledge without presenting guesses as facts.',
    )
    ..writeln()
    ..writeln(
      'Presentation title: ${title == null ? '(untitled)' : _inline(title)}',
    )
    ..writeln('Presentation script:')
    ..write(script);
  return buffer.toString();
}

/// AUTO adds no rule: the model matches the presentation. Any other value is
/// inlined as typed.
String? _toneRule(String tone) => switch (_collapse(tone).toUpperCase()) {
  'AUTO' => null,
  'NATURAL' => '- Tone: natural and conversational.',
  'FORMAL' => '- Tone: formal and polished.',
  _ => '- Tone: ${_inline(tone)}.',
};

/// Unicode White_Space plus the BOM, spelled out rather than left to `\s`:
/// the server's regex `\s` is ASCII-only, and a U+2028 left in a line would let
/// a crafted value start a rule line of its own.
final RegExp _whitespace = RegExp(
  r'[\t\n\v\f\r \u0085   -     　﻿]+',
);

/// Caller-supplied free strings inlined into single prompt lines. The script
/// is exempt: it is free-form grounding content.
String _inline(String value) {
  final collapsed = _collapse(value);
  return collapsed.length > _maxInline
      ? collapsed.substring(0, _maxInline)
      : collapsed;
}

/// UTF-16 units, as both languages count them.
const int _maxInline = 100;

String _collapse(String value) {
  var collapsed = value.replaceAll(_whitespace, ' ');
  if (collapsed.startsWith(' ')) collapsed = collapsed.substring(1);
  if (collapsed.endsWith(' ')) {
    collapsed = collapsed.substring(0, collapsed.length - 1);
  }
  return collapsed;
}
