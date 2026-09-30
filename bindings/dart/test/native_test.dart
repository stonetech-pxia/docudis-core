// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

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

void main() {
  final path = _library();
  test(
    'real C ABI detects, processes, restores, copies, and frees buffers',
    () {
      final rust = DocudisNative.open(path);
      expect(rust.abiVersion, DocudisNative.expectedAbiVersion);
      const text = '😀Alice: alice@example.com';
      final request = <String, Object?>{
        'schema_version': 1,
        'text': text,
        'regions': <String>[],
        'dictionary': ['Alice'],
        'detections': <Object?>[],
      };
      final detected = rust.detect(request);
      final detections = detected['detections']! as List<Object?>;
      expect(
        detections.map((d) => (d! as Map)['value']),
        containsAll(['Alice', 'alice@example.com']),
      );
      expect(
        (detections.first! as Map)['start'],
        2,
      ); // Dart UTF-16 after emoji.

      final processed = rust.process(request);
      expect(processed['text'], '😀[CUSTOM_1]: [EMAIL_1]');
      expect((processed['detections']! as List<Object?>), hasLength(2));
      final restored = rust.restore({
        'schema_version': 1,
        'text': processed['text'],
        'mappings': processed['mappings'],
      });
      expect(restored['text'], text);
    },
    skip: path == null
        ? 'Build docudis-capi first or set DOCUDIS_LIBRARY'
        : false,
  );

  test(
    'stable error code becomes a Dart exception',
    () {
      final rust = DocudisNative.open(path);
      expect(
        () => rust.restore({
          'schema_version': 99,
          'text': 'x',
          'mappings': <Object?>[],
        }),
        throwsA(
          isA<DocudisException>().having(
            (e) => e.status,
            'status',
            DocudisStatus.invalidArgument,
          ),
        ),
      );
    },
    skip: path == null
        ? 'Build docudis-capi first or set DOCUDIS_LIBRARY'
        : false,
  );

  test(
    'differential fallback diagnostics never include sensitive payloads',
    () async {
      final rust = DocudisNative.open(path);
      Map<String, Object?>? diagnostic;
      final runner = DocudisDifferentialRunner(
        rust,
        onMismatch: (_, details) => diagnostic = details,
      );
      final result = await runner.process(
        dartReference: () async => <String, Object?>{
          'schema_version': 1,
          'text': 'reference-result',
          'detections': <Object?>[],
          'mappings': <Object?>[],
        },
        rustRequest: <String, Object?>{
          'schema_version': 1,
          'text': 'Secret Alice alice@example.com',
          'regions': <String>[],
          'dictionary': <String>[],
          'detections': <Object?>[],
        },
      );
      expect(result['text'], 'reference-result');
      final encoded = diagnostic.toString();
      expect(encoded, isNot(contains('Secret')));
      expect(encoded, isNot(contains('Alice')));
      expect(encoded, isNot(contains('alice@example.com')));
      expect(diagnostic, contains('case_digest'));
    },
    skip: path == null
        ? 'Build docudis-capi first or set DOCUDIS_LIBRARY'
        : false,
  );

  test('missing native library has an actionable load failure', () {
    expect(
      () => DocudisNative.open('/definitely/missing/libdocudis_capi.so'),
      throwsA(anything),
    );
  });
}
