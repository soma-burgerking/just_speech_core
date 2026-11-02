/// Script rules the web (`script/ts`) and the app must apply alike: the counts
/// are sent to the server, which stores them as given, and word indices address
/// slide mappings. Both are pinned to `script/vectors`.
library;

/// A sentence terminator plus the trailing quotes/brackets that still count as
/// part of the ending — the web teleprompter's line rule.
const String sentenceEndSource = r'''[.?!…。！？][")'’”\]】』」]*''';

final RegExp _sentenceBoundary = RegExp('(?<=$sentenceEndSource)\\s+|\\n+');
final RegExp _whitespace = RegExp(r'\s+');

/// JavaScript's `trim() !== ""`. Dart's `trim` also drops U+0085, which
/// JavaScript keeps.
final RegExp _visible = RegExp(r'\S');

/// Words as the server defines them: split on `\s+`, empty pieces dropped.
int countScriptTokens(String script) =>
    script.split(_whitespace).where((token) => token.isNotEmpty).length;

/// Sentences, for `scriptLineCount`: a break after a sentence end, or a line
/// break.
int countScriptLines(String script) =>
    script.split(_sentenceBoundary).where(_visible.hasMatch).length;
