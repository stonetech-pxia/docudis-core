// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'dart:convert';
import 'dart:io';

import 'package:docudis_ffi/docudis_ffi.dart';
import 'package:test/test.dart';

String? _library() {
  final explicit = Platform.environment['DOCUDIS_LIBRARY'];
  if (explicit != null) return explicit;
  for (final path in [
    '../../target/debug/libdocudis_capi.dylib',
    '../../target/debug/libdocudis_capi.so',
    r'..\..\target\debug\docudis_capi.dll',
  ]) {
    if (File(path).existsSync()) return File(path).absolute.path;
  }
  return null;
}

Map<String, Object?> _fixture(String name) => (jsonDecode(
  File('../../conformance/fixtures/v1/$name').readAsStringSync(),
) as Map).cast<String, Object?>();

List<Map<String, Object?>> _cases(Map<String, Object?> fixture) => [
  for (final c in fixture['cases']! as List<Object?>)
    (c! as Map).cast<String, Object?>(),
];

Detection _detection(Map<String, Object?> json) => Detection.fromJson({
  ...json,
  'start': json['start_utf16'],
  'end': json['end_utf16'],
});

void main() {
  final path = _library();
  final skip = path == null
      ? 'Build docudis-capi first or set DOCUDIS_LIBRARY'
      : false;
  late final DocudisCore core;
  setUpAll(() {
    if (path != null) core = DocudisCore.open(path);
  });

  test('chunks reproduce the Dart reference in UTF-16 offsets', () {
    for (final c in _cases(_fixture('chunks.json'))) {
      final text = c['text']! as String;
      final taken = [
        for (final r in (c['taken']! as List<Object?>).cast<Map>())
          Detection(
            type: EntityType.other,
            value: text.substring(
              r['start_utf16'] as int,
              r['end_utf16'] as int,
            ),
            start: r['start_utf16'] as int,
            end: r['end_utf16'] as int,
            confidence: 1,
            detector: 'fixture',
            source: DetectionSource.manual,
          ),
      ];
      expect(core.chunk(text, taken: taken), [
        for (final r in (c['chunks']! as List<Object?>).cast<Map>())
          TextChunk(r['start_utf16'] as int, r['end_utf16'] as int),
      ], reason: c['name'] as String);
    }
  }, skip: skip);

  test('merge reproduces the Dart reference in UTF-16 offsets', () {
    for (final c in _cases(_fixture('merge.json'))) {
      final text = c['text']! as String;
      List<Map<String, Object?>> json(List<Detection> ds) => [
        for (final d in ds) d.toJson(),
      ];
      final input = [
        for (final d in (c['detections']! as List<Object?>).cast<Map>())
          _detection(d.cast()),
      ];
      final expected = [
        for (final d in (c['merged']! as List<Object?>).cast<Map>())
          _detection(d.cast()),
      ];
      expect(
        json(core.merge(text, input)),
        json(expected),
        reason: c['name'] as String,
      );
    }
  }, skip: skip);

  test('regions reproduce the Dart reference', () {
    for (final c in _cases(_fixture('regions.json'))) {
      expect(
        core.regions(
          (c['languages']! as List<Object?>).cast<String>(),
          c['text']! as String,
        ),
        (c['regions']! as List<Object?>).cast<String>().toSet(),
        reason: c['name'] as String,
      );
    }
  }, skip: skip);

  test('reply checks reproduce the Dart reference', () {
    final fixture = _fixture('reply_check.json');
    final candidates = {
      for (final d in (fixture['candidates']! as List<Object?>).cast<Map>())
        d['id'] as String: ReplyCandidate(
          output: d['text'] as String,
          map: PlaceholderMap([
            for (final p in (d['placeholders'] as List<Object?>).cast<String>())
              MappingEntry(
                original: 'original of $p',
                placeholder: p,
                type: EntityType.other,
              ),
          ]),
        ),
    };
    for (final c in _cases(fixture)) {
      final check = core.replyCheck(
        c['reply']! as String,
        c['id']! as String,
        candidates,
      );
      final name = c['name'] as String;
      expect(check.unknown, c['unknown'], reason: name);
      expect(check.invented, c['invented'], reason: name);
      expect(check.betterMatch, c['better_match'], reason: name);
    }
  }, skip: skip);

  test('languages are identified when the library has them', () {
    try {
      final languages = core.languages('租客每月通过银行转账支付房租。');
      expect(languages, ['zh']);
      expect(core.regions(languages, '租客'), {'cn'});
    } on DocudisException catch (e) {
      expect(e.status, DocudisStatus.coreError);
      expect(e.message, contains('language-id'));
      markTestSkipped('library built without the language-id feature');
    }
  }, skip: skip);

  test('process, review edits, reapply and restore stay typed', () {
    const text = '😀 Alice Martin: alice@example.com. Alice Martin left.';
    final first = core.process(
      text,
      regions: const {},
      detections: [
        Detection(
          type: EntityType.person,
          value: 'Alice Martin',
          start: 3,
          end: 15,
          confidence: 0.9,
          detector: 'ner:test',
          source: DetectionSource.model,
        ),
      ],
    );
    expect(first.result.text, '😀 [PERSON_1]: [EMAIL_1]. [PERSON_1] left.');
    expect(first.detections.map((d) => (d.value, d.start, d.source)), [
      ('Alice Martin', 3, DetectionSource.model),
      ('alice@example.com', 17, DetectionSource.strongRule),
      ('Alice Martin', 36, DetectionSource.propagated),
    ]);
    expect(first.result.replacements.first, (
      start: 3,
      end: 15,
      placeholder: '[PERSON_1]',
    ));

    // The user shows the e-mail again: merge, then reanonymize with the
    // stored map so [PERSON_1] keeps its number.
    final edited = core.merge(text, [
      for (final d in first.detections)
        d.type == EntityType.email ? d.copyWith(enabled: false) : d,
    ]);
    final again = core.anonymize(text, edited, previous: first.result.map);
    expect(again.text, '😀 [PERSON_1]: alice@example.com. [PERSON_1] left.');

    final chunks = core.chunk(text, taken: edited.where((d) => d.enabled));
    expect(
      [for (final c in chunks) text.substring(c.start, c.end)],
      ['alice@example.com', 'left'],
    );

    expect(core.restore('Hi [person 1]', again.map), 'Hi Alice Martin');
  }, skip: skip);

  test('PlaceholderMap reads and writes the stored record format', () {
    const stored =
        '[{"original":"Alice Martin","placeholder":"[PERSON_1]","type":"PERSON"},'
        '{"original":"Alice","placeholder":"[PERSON_1]","type":"PERSON"},'
        '{"original":"alice@example.com","placeholder":"[EMAIL_1]","type":"EMAIL"}]';
    final map = PlaceholderMap.fromJson(stored);
    expect(map.toJson(), stored);
    expect(map.length, 2);
    expect(map.reverse, {
      '[PERSON_1]': 'Alice Martin',
      '[EMAIL_1]': 'alice@example.com',
    });
  });
}
