import 'dart:convert';
import 'dart:io';

import 'package:just_speech_qa_prompt/qa_prompt.dart';
import 'package:test/test.dart';

void main() {
  final cases = jsonDecode(
    File('../vectors/cases.json').readAsStringSync(),
  ) as List<dynamic>;

  for (final c in cases.cast<Map<String, dynamic>>()) {
    test(c['name'], () {
      expect(
        qaSystemPrompt(
          title: c['title'] as String?,
          language: c['language'] as String,
          tone: c['tone'] as String,
          maxLines: c['maxLines'] as int?,
          script: c['script'] as String,
        ),
        c['prompt'],
      );
    });
  }
}
