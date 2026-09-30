// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'dart:convert';

import 'binding.dart';

typedef DifferentialDiagnostic = void Function(
  String message,
  Map<String, Object?> details,
);

class DocudisDifferentialRunner {
  const DocudisDifferentialRunner(this.rust, {this.onMismatch});
  final DocudisNative rust;
  final DifferentialDiagnostic? onMismatch;

  /// Runs the Dart reference and Rust candidate. [rustRequest] may contain the
  /// NER detections already produced by Dart, in Dart UTF-16 offsets. Until a
  /// caller explicitly switches modes, mismatch and Rust failure both return
  /// the Dart result.
  Future<Map<String, Object?>> process({
    required Future<Map<String, Object?>> Function() dartReference,
    required Map<String, Object?> rustRequest,
  }) async {
    final dart = await dartReference();
    try {
      final candidate = rust.process(rustRequest);
      if (_canonical(candidate) != _canonical(dart)) {
        onMismatch?.call('Dart/Rust anonymization mismatch', {
          'case_digest': _digest(rustRequest),
          'input_utf16_length': (rustRequest['text'] as String?)?.length,
          'dart_detection_count': _count(dart, 'detections'),
          'rust_detection_count': _count(candidate, 'detections'),
          'dart_mapping_count': _count(dart, 'mappings'),
          'rust_mapping_count': _count(candidate, 'mappings'),
        });
        return dart;
      }
      return candidate;
    } on Object catch (error, stackTrace) {
      onMismatch?.call('Rust candidate failed; using Dart result', {
        'case_digest': _digest(rustRequest),
        'input_utf16_length': (rustRequest['text'] as String?)?.length,
        'error_type': error.runtimeType.toString(),
        if (error is DocudisException) 'error_status': error.status.name,
        'stack_digest': _digest({'stack': '$stackTrace'}),
      });
      return dart;
    }
  }

  String _canonical(Map<String, Object?> value) => jsonEncode(value);

  int _count(Map<String, Object?> value, String key) =>
      (value[key] as List<Object?>?)?.length ?? 0;

  /// A deterministic, non-reversible diagnostic id. It deliberately hashes
  /// only request shape and lengths, never source text or detected values.
  String _digest(Map<String, Object?> request) {
    final text = request['text'] as String? ?? '';
    final safe = jsonEncode({
      'length': text.length,
      'regions': (request['regions'] as List<Object?>?)?.length ?? 0,
      'dictionary': (request['dictionary'] as List<Object?>?)?.length ?? 0,
      'detections': (request['detections'] as List<Object?>?)?.length ?? 0,
      'salt': 0x446f6375,
    });
    var hash = 0xcbf29ce484222325;
    for (final byte in utf8.encode(safe)) {
      hash ^= byte;
      hash = (hash * 0x100000001b3) & 0xffffffffffffffff;
    }
    return hash.toRadixString(16).padLeft(16, '0');
  }
}
