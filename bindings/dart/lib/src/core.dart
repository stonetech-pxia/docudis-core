// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'binding.dart';
import 'models.dart';

/// Typed access to the Rust core: everything an app needs to detect,
/// anonymize, review and restore, with Dart (UTF-16) offsets throughout.
class DocudisCore {
  DocudisCore(this.native);

  /// Opens the native library; see [DocudisNative.open].
  factory DocudisCore.open([String? path]) =>
      DocudisCore(DocudisNative.open(path));

  final DocudisNative native;

  /// Bundled rules (only [regions]' packs plus the universal one when given),
  /// the user's [dictionary] and, optionally, the bundled company and place
  /// lists, merged with the caller's own [detections] (model spans).
  List<Detection> detect(
    String text, {
    Set<String>? regions,
    List<String> dictionary = const [],
    List<String> neverHide = const [],
    bool includeBundledLists = false,
    List<Detection> detections = const [],
  }) => _detections(
    native.detect(
      _detectRequest(
        text,
        regions,
        dictionary,
        neverHide,
        includeBundledLists,
        detections,
      ),
    ),
  );

  /// [detect] and [anonymize] in one call.
  ({List<Detection> detections, AnonymizedText result}) process(
    String text, {
    Set<String>? regions,
    List<String> dictionary = const [],
    List<String> neverHide = const [],
    bool includeBundledLists = false,
    List<Detection> detections = const [],
    PlaceholderMap? previous,
  }) {
    final response = native.process({
      ..._detectRequest(
        text,
        regions,
        dictionary,
        neverHide,
        includeBundledLists,
        detections,
      ),
      if (previous != null) 'previous_map': _entries(previous),
    });
    return (detections: _detections(response), result: _anonymized(response));
  }

  /// Replaces the enabled [detections] in [text] with typed placeholders.
  /// With [previous] (the map of an earlier run on the same text), values
  /// keep their placeholders and new ones are numbered after them.
  AnonymizedText anonymize(
    String text,
    List<Detection> detections, {
    PlaceholderMap? previous,
  }) => _anonymized(
    native.anonymize({
      'text': text,
      'detections': [for (final d in detections) d.toJson()],
      if (previous != null) 'previous_map': _entries(previous),
    }),
  );

  /// Resolves overlaps and propagates values among [detections] without
  /// detecting anything new: what the review page's edits go through.
  List<Detection> merge(String text, List<Detection> detections) => _detections(
    native.merge({
      'text': text,
      'detections': [for (final d in detections) d.toJson()],
    }),
  );

  /// The review page's one-tap chunks; none crosses a [taken] span.
  List<TextChunk> chunk(String text, {Iterable<Detection> taken = const []}) {
    final response = native.chunk({
      'text': text,
      'taken': [
        for (final d in taken) {'start': d.start, 'end': d.end},
      ],
    });
    return [
      for (final c in (response['chunks']! as List<Object?>).cast<Map>())
        TextChunk(c['start']! as int, c['end']! as int),
    ];
  }

  /// Rule-pack regions for [text] in these BCP-47 [languages].
  Set<String> regions(Iterable<String> languages, String text) {
    final response = native.regions({
      'languages': languages.toList(),
      'text': text,
    });
    return {
      for (final r in response['regions']! as List<Object?>) r! as String,
    };
  }

  /// Whether [reply] answers document [id] among [candidates] (record id ->
  /// document), or another one fits it clearly better.
  ReplyCheck replyCheck(
    String reply,
    String id,
    Map<String, ReplyCandidate> candidates,
  ) {
    final response = native.replyCheck({
      'reply': reply,
      'id': id,
      'candidates': [
        for (final MapEntry(key: id, value: c) in candidates.entries)
          {'id': id, 'text': c.output, 'placeholders': c.placeholders},
      ],
    });
    return ReplyCheck(
      unknown: _strings(response['unknown']),
      invented: _strings(response['invented']),
      betterMatch: response['better_match'] as String?,
    );
  }

  /// Puts the originals back into [text], including placeholders an AI
  /// mangled when only one entry of [map] fits.
  String restore(String text, PlaceholderMap map) =>
      native.restore({
            'schema_version': 1,
            'text': text,
            'mappings': _entries(map),
          })['text']!
          as String;

  static Map<String, Object?> _detectRequest(
    String text,
    Set<String>? regions,
    List<String> dictionary,
    List<String> neverHide,
    bool includeBundledLists,
    List<Detection> detections,
  ) => {
    'text': text,
    if (regions != null) 'regions': regions.toList(),
    'dictionary': dictionary,
    'never_hide': neverHide,
    'include_bundled_lists': includeBundledLists,
    'detections': [for (final d in detections) d.toJson()],
  };

  static List<Map<String, Object?>> _entries(PlaceholderMap map) => [
    for (final e in map.entries) e.toJson(),
  ];

  static List<Detection> _detections(Map<String, Object?> response) => [
    for (final d in response['detections']! as List<Object?>)
      Detection.fromJson((d! as Map).cast<String, Object?>()),
  ];

  static AnonymizedText _anonymized(Map<String, Object?> response) =>
      AnonymizedText(
        text: response['text']! as String,
        map: PlaceholderMap([
          for (final e in response['mappings']! as List<Object?>)
            MappingEntry.fromJson((e! as Map).cast<String, Object?>()),
        ]),
        replacements: [
          for (final r
              in (response['replacements']! as List<Object?>).cast<Map>())
            (
              start: r['start']! as int,
              end: r['end']! as int,
              placeholder: r['placeholder']! as String,
            ),
        ],
      );

  static List<String> _strings(Object? list) => [
    for (final s in list! as List<Object?>) s! as String,
  ];
}
