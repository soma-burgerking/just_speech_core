import 'dart:convert';
import 'dart:io';

import 'package:just_speech_script/script.dart';
import 'package:test/test.dart';

void main() {
  final cases =
      jsonDecode(File('../vectors/cases.json').readAsStringSync())
          as List<dynamic>;

  for (final c in cases.cast<Map<String, dynamic>>()) {
    test(c['name'], () {
      final script = c['script'] as String;
      expect(countScriptTokens(script), c['tokens'], reason: 'tokens');
      expect(countScriptLines(script), c['lines'], reason: 'lines');
    });
  }
}
