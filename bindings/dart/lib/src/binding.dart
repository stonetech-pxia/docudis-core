// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'offsets.dart';

final class _Buffer extends Struct {
  external Pointer<Uint8> ptr;
  @Size()
  external int len;
  @Size()
  external int capacity;
}

typedef _CallNative = Int32 Function(Pointer<Uint8>, Size, Pointer<_Buffer>);
typedef _CallDart = int Function(Pointer<Uint8>, int, Pointer<_Buffer>);
typedef _FreeNative = Void Function(Pointer<_Buffer>);
typedef _FreeDart = void Function(Pointer<_Buffer>);
typedef _ErrorNative = Pointer<Char> Function();
typedef _ErrorDart = Pointer<Char> Function();
typedef _VersionNative = Uint32 Function();
typedef _VersionDart = int Function();

enum DocudisStatus {
  ok,
  invalidArgument,
  invalidUtf8,
  invalidJson,
  coreError,
  panic,
  unknown,
}

class DocudisException implements Exception {
  const DocudisException(this.status, this.message);
  final DocudisStatus status;
  final String message;
  @override
  String toString() => 'DocudisException(${status.name}): $message';
}

class DocudisNative {
  DocudisNative._(DynamicLibrary library)
    : _abiVersion = library.lookupFunction<_VersionNative, _VersionDart>(
        'docudis_v1_abi_version',
      ),
      _anonymize = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_anonymize_json',
      ),
      _detect = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_detect_json',
      ),
      _process = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_process_json',
      ),
      _restore = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_restore_json',
      ),
      _chunk = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_chunk_json',
      ),
      _merge = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_merge_json',
      ),
      _regions = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_regions_json',
      ),
      _languages = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_languages_json',
      ),
      _replyCheck = library.lookupFunction<_CallNative, _CallDart>(
        'docudis_v1_reply_check_json',
      ),
      _free = library.lookupFunction<_FreeNative, _FreeDart>(
        'docudis_v1_buffer_free',
      ),
      _lastError = library.lookupFunction<_ErrorNative, _ErrorDart>(
        'docudis_v1_last_error_message',
      ) {
    final actual = _abiVersion();
    if (actual != expectedAbiVersion) {
      throw DocudisException(
        DocudisStatus.invalidArgument,
        'incompatible Docudis ABI $actual; expected $expectedAbiVersion',
      );
    }
  }

  factory DocudisNative.open([String? path]) =>
      DocudisNative._(DynamicLibrary.open(path ?? defaultLibraryName));

  static String get defaultLibraryName {
    if (Platform.isMacOS || Platform.isIOS) return 'libdocudis_capi.dylib';
    if (Platform.isWindows) return 'docudis_capi.dll';
    return 'libdocudis_capi.so';
  }

  static const expectedAbiVersion = 1;

  final _VersionDart _abiVersion;
  final _CallDart _anonymize;
  final _CallDart _detect;
  final _CallDart _process;
  final _CallDart _restore;
  final _CallDart _chunk;
  final _CallDart _merge;
  final _CallDart _regions;
  final _CallDart _languages;
  final _CallDart _replyCheck;
  final _FreeDart _free;
  final _ErrorDart _lastError;

  int get abiVersion => _abiVersion();

  Map<String, Object?> anonymize(
    Map<String, Object?> request, {
    bool offsetsAreUtf16 = true,
  }) => _call(
    _anonymize,
    _request(request, offsetsAreUtf16),
    convertResponseOffsets: offsetsAreUtf16,
  );

  Map<String, Object?> detect(
    Map<String, Object?> request, {
    bool offsetsAreUtf16 = true,
  }) => _call(
    _detect,
    _request(request, offsetsAreUtf16),
    convertResponseOffsets: offsetsAreUtf16,
  );

  Map<String, Object?> process(
    Map<String, Object?> request, {
    bool offsetsAreUtf16 = true,
  }) => _call(
    _process,
    _request(request, offsetsAreUtf16),
    convertResponseOffsets: offsetsAreUtf16,
  );

  Map<String, Object?> restore(Map<String, Object?> request) =>
      _call(_restore, request);

  /// `{text, taken: [{start, end}]}` -> `{chunks: [{start, end}]}`.
  Map<String, Object?> chunk(
    Map<String, Object?> request, {
    bool offsetsAreUtf16 = true,
  }) => _call(
    _chunk,
    _request(request, offsetsAreUtf16),
    convertResponseOffsets: offsetsAreUtf16,
  );

  /// `{text, detections}` -> `{detections}`, without detecting anything new.
  Map<String, Object?> merge(
    Map<String, Object?> request, {
    bool offsetsAreUtf16 = true,
  }) => _call(
    _merge,
    _request(request, offsetsAreUtf16),
    convertResponseOffsets: offsetsAreUtf16,
  );

  /// `{languages, text}` -> `{regions}`.
  Map<String, Object?> regions(Map<String, Object?> request) =>
      _call(_regions, {'schema_version': 1, ...request});

  /// `{text}` -> `{languages}`. Throws a [DocudisException] with
  /// [DocudisStatus.coreError] when the library was built without the
  /// `language-id` feature.
  Map<String, Object?> languages(Map<String, Object?> request) =>
      _call(_languages, {'schema_version': 1, ...request});

  /// `{reply, id, candidates: [{id, text, placeholders}]}` ->
  /// `{unknown, invented, better_match}`.
  Map<String, Object?> replyCheck(Map<String, Object?> request) =>
      _call(_replyCheck, {'schema_version': 1, ...request});

  Map<String, Object?> _request(Map<String, Object?> request, bool convert) {
    final text = request['text']! as String;
    final detections = request['detections'] as List<Object?>?;
    final taken = request['taken'] as List<Object?>?;
    final policy = (request['policy'] as Map?)?.cast<String, Object?>();
    final ranges = policy?['ranges'] as List<Object?>?;
    final index =
        convert && (detections != null || taken != null || ranges != null)
        ? OffsetIndex(text)
        : null;
    return {
      'schema_version': 1,
      ...request,
      if (index != null && detections != null)
        'detections': [
          for (final d in detections)
            _rangeToUtf8(index, (d! as Map).cast<String, Object?>()),
        ],
      if (index != null && taken != null)
        'taken': [
          for (final r in taken)
            _rangeToUtf8(index, (r! as Map).cast<String, Object?>()),
        ],
      if (index != null && ranges != null)
        'policy': {
          ...policy!,
          'ranges': [
            for (final r in ranges.cast<List<Object?>>())
              [index.toUtf8(r[0]! as int), index.toUtf8(r[1]! as int)],
          ],
        },
    };
  }

  Map<String, Object?> _call(
    _CallDart call,
    Map<String, Object?> request, {
    bool convertResponseOffsets = false,
  }) {
    final encoded = utf8.encode(jsonEncode(request));
    final input = calloc<Uint8>(encoded.length);
    final output = calloc<_Buffer>();
    try {
      input.asTypedList(encoded.length).setAll(0, encoded);
      final statusCode = call(input, encoded.length, output);
      if (statusCode != 0) {
        final message = _lastError().cast<Utf8>().toDartString();
        throw DocudisException(_status(statusCode), message);
      }
      // Copy before handing the allocation back to Rust.
      final bytes = Uint8List.fromList(
        output.ref.ptr.asTypedList(output.ref.len),
      );
      final response = (jsonDecode(utf8.decode(bytes)) as Map)
          .cast<String, Object?>();
      if (convertResponseOffsets) {
        _convertResponse(response, request['text']! as String);
      }
      return response;
    } finally {
      if (output.ref.ptr.address != 0) {
        _free(output);
      }
      calloc.free(output);
      calloc.free(input);
    }
  }

  /// Turns every `start`/`end` range the response carries (detections,
  /// replacements, chunks) into UTF-16 offsets, indexing the text once.
  static void _convertResponse(Map<String, Object?> response, String text) {
    OffsetIndex? index;
    for (final key in const ['detections', 'replacements', 'chunks']) {
      final ranges = response[key] as List<Object?>?;
      if (ranges == null) continue;
      index ??= OffsetIndex(text);
      response[key] = [
        for (final r in ranges)
          _rangeToUtf16(index, (r! as Map).cast<String, Object?>()),
      ];
    }
  }

  static Map<String, Object?> _rangeToUtf8(
    OffsetIndex index,
    Map<String, Object?> range,
  ) => {
    ...range,
    'start': index.toUtf8(range['start']! as int),
    'end': index.toUtf8(range['end']! as int),
  };

  static Map<String, Object?> _rangeToUtf16(
    OffsetIndex index,
    Map<String, Object?> range,
  ) => {
    ...range,
    'start': index.toUtf16(range['start']! as int),
    'end': index.toUtf16(range['end']! as int),
  };

  static DocudisStatus _status(int code) => switch (code) {
    0 => DocudisStatus.ok,
    1 => DocudisStatus.invalidArgument,
    2 => DocudisStatus.invalidUtf8,
    3 => DocudisStatus.invalidJson,
    4 => DocudisStatus.coreError,
    255 => DocudisStatus.panic,
    _ => DocudisStatus.unknown,
  };
}
