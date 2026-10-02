// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'dart:convert';

/// Kinds of sensitive information Docudis detects, mirroring the Rust
/// `EntityType`.
///
/// [placeholderName] is the stable, English, uppercase label used inside
/// placeholders such as `[PERSON_1]`; it never changes with the UI locale.
enum EntityType {
  person('PERSON'),
  email('EMAIL'),
  phone('PHONE'),

  /// Government identifiers: national ID cards, passports, SSN-like numbers.
  id('ID'),

  /// A digit string a loose rule matched: some identifier, but claiming a
  /// specific kind would be a guess that misleads whoever reads the output.
  number('NUMBER'),
  card('CARD'),
  iban('IBAN'),

  /// Detected but left visible by default, like [amount].
  date('DATE'),

  /// A [date] that follows a "born" / "date of birth" label.
  birthDate('BIRTH_DATE'),
  amount('AMOUNT'),
  ip('IP'),
  url('URL'),
  address('ADDRESS'),
  company('COMPANY'),
  secret('SECRET'),
  apiKey('API_KEY'),

  /// User-supplied dictionary terms.
  custom('CUSTOM'),
  other('OTHER');

  const EntityType(this.placeholderName);

  final String placeholderName;

  /// Resolves a placeholder label or a legacy rule-pack `entityType` name.
  static EntityType? fromName(String name) {
    switch (name) {
      case 'SSN':
        return EntityType.id;
      case 'CREDIT_CARD':
        return EntityType.card;
      case 'CURRENCY':
        return EntityType.amount;
      case 'IP_ADDRESS':
        return EntityType.ip;
    }
    for (final t in EntityType.values) {
      if (t.placeholderName == name) return t;
    }
    return null;
  }
}

/// Where a detection came from; Rust decides who wins when spans overlap,
/// higher [priority] first.
enum DetectionSource {
  dictionary(40),
  validatedRule(30),
  strongRule(25),
  model(20),
  bundledList(20),
  rule(10),
  propagated(5),

  /// A span the user selected by hand.
  manual(50);

  const DetectionSource(this.priority);

  final int priority;
}

/// One sensitive span inside a text. Offsets are UTF-16 code-unit indices
/// into the text it was detected in (Dart `String` indices); the binding
/// converts them to and from Rust's UTF-8 offsets.
class Detection {
  Detection({
    required this.type,
    required this.value,
    required this.start,
    required this.end,
    required this.confidence,
    required this.detector,
    required this.source,
    this.enabled = true,
  }) : assert(end > start);

  final EntityType type;
  final String value;
  final int start;
  final int end;
  final double confidence;

  /// Detector id, e.g. `regex:cn:phone_mobile`, `ner:xlmr`, `dictionary`.
  final String detector;
  final DetectionSource source;

  /// Whether the user wants this span replaced.
  final bool enabled;

  int get length => end - start;

  bool overlaps(Detection other) => start < other.end && end > other.start;

  Detection copyWith({bool? enabled, EntityType? type}) => Detection(
    type: type ?? this.type,
    value: value,
    start: start,
    end: end,
    confidence: confidence,
    detector: detector,
    source: source,
    enabled: enabled ?? this.enabled,
  );

  /// The schema-v1 detection object, with UTF-16 offsets.
  Map<String, Object?> toJson() => {
    'type': type.placeholderName,
    'value': value,
    'start': start,
    'end': end,
    'confidence': confidence,
    'detector': detector,
    'source': source.name,
    'enabled': enabled,
  };

  factory Detection.fromJson(Map<String, Object?> json) => Detection(
    type: EntityType.fromName(json['type']! as String) ?? EntityType.other,
    value: json['value']! as String,
    start: json['start']! as int,
    end: json['end']! as int,
    confidence: (json['confidence']! as num).toDouble(),
    detector: json['detector']! as String,
    source: DetectionSource.values.byName(json['source']! as String),
    enabled: json['enabled'] as bool? ?? true,
  );

  @override
  String toString() =>
      '${type.placeholderName}[$start:$end] ${confidence.toStringAsFixed(2)} $detector';
}

/// What to do with every detection of one [EntityType] (see
/// [DetectionPolicy]).
enum TypeAction {
  /// Hide it, even where the defaults would only show it (dates, amounts).
  hide,

  /// Detect it and let it win overlaps, but leave it visible.
  keep,

  /// Ignore it entirely, so it cannot displace other detections.
  off,
}

/// What the user wants hidden, whichever detector or model found it.
/// [types] does not apply to dictionary terms or manual spans; [ranges]
/// applies to everything.
class DetectionPolicy {
  const DetectionPolicy({this.types = const {}, this.ranges});

  final Map<EntityType, TypeAction> types;

  /// Half-open UTF-16 ranges of the text to process; `null` processes all
  /// of it. A detection touching any range is kept whole, and nothing is
  /// propagated outside them. Must not be empty.
  final List<({int start, int end})>? ranges;

  /// The schema-v1 policy object, with UTF-16 offsets.
  Map<String, Object?> toJson() => {
    'types': {
      for (final e in types.entries) e.key.placeholderName: e.value.name,
    },
    if (ranges != null)
      'ranges': [
        for (final r in ranges!) [r.start, r.end],
      ],
  };
}

/// One reversible mapping: an original value and the placeholder it became.
class MappingEntry {
  const MappingEntry({
    required this.original,
    required this.placeholder,
    required this.type,
  });

  final String original;
  final String placeholder;
  final EntityType type;

  Map<String, Object?> toJson() => {
    'original': original,
    'placeholder': placeholder,
    'type': type.placeholderName,
  };

  factory MappingEntry.fromJson(Map<String, Object?> json) => MappingEntry(
    original: json['original']! as String,
    placeholder: json['placeholder']! as String,
    type: EntityType.fromName(json['type']! as String) ?? EntityType.other,
  );
}

/// The key that restores an anonymized text: which original became which
/// placeholder. Rust issues the placeholders and restores replies
/// ([DocudisCore.anonymize], [DocudisCore.restore]); this class only holds
/// the entries, in the order they were issued.
class PlaceholderMap {
  PlaceholderMap([Iterable<MappingEntry> entries = const []]) {
    for (final e in entries) {
      _byOriginal[e.original] = e;
    }
  }

  /// Original -> entry. A repeated original keeps its first position and
  /// its last placeholder, as the Rust map imports it.
  final _byOriginal = <String, MappingEntry>{};

  bool get isEmpty => _byOriginal.isEmpty;

  int get length => reverse.length;

  List<MappingEntry> get entries => List.unmodifiable(_byOriginal.values);

  /// Placeholder -> canonical (longest) original, one row per placeholder.
  Map<String, String> get reverse {
    final reverse = <String, String>{};
    for (final e in _byOriginal.values) {
      final canonical = reverse[e.placeholder];
      if (canonical == null || e.original.length > canonical.length) {
        reverse[e.placeholder] = e.original;
      }
    }
    return Map.unmodifiable(reverse);
  }

  /// The JSON list Android's record store keeps: `[{original, placeholder,
  /// type}]`.
  String toJson() => jsonEncode([for (final e in entries) e.toJson()]);

  factory PlaceholderMap.fromJson(String json) => PlaceholderMap([
    for (final e in jsonDecode(json) as List<Object?>)
      MappingEntry.fromJson((e! as Map).cast<String, Object?>()),
  ]);
}

/// Output of applying detections to a text.
class AnonymizedText {
  const AnonymizedText({
    required this.text,
    required this.map,
    this.replacements = const [],
  });

  /// The text with every enabled detection replaced by a placeholder.
  final String text;

  /// Placeholder <-> original mapping needed to restore replies.
  final PlaceholderMap map;

  /// Which characters of the original went where, in reading order:
  /// `original[start, end)` became `placeholder`, in UTF-16 offsets.
  final List<({int start, int end, String placeholder})> replacements;
}

/// A range of the text the user can redact with a single tap, in UTF-16
/// code units.
class TextChunk {
  const TextChunk(this.start, this.end);

  final int start;
  final int end;

  int get length => end - start;

  bool contains(int offset) => offset >= start && offset < end;

  @override
  bool operator ==(Object other) =>
      other is TextChunk && other.start == start && other.end == end;

  @override
  int get hashCode => Object.hash(start, end);

  @override
  String toString() => 'TextChunk($start:$end)';
}

/// One document an AI reply may answer: the anonymized text that was sent
/// and the placeholders its key restores.
class ReplyCandidate {
  ReplyCandidate({required this.output, required PlaceholderMap map})
    : placeholders = map.reverse.keys.toList(growable: false);

  final String output;
  final List<String> placeholders;
}

/// What a pasted reply says about the document it is restored with.
class ReplyCheck {
  const ReplyCheck({
    this.unknown = const [],
    this.invented = const [],
    this.betterMatch,
  });

  /// Labels in the reply the document never issued, as `[PERSON_4]`, in
  /// reading order, except the [invented] ones.
  final List<String> unknown;

  /// Labels an AI plausibly made up: a higher number of a type the
  /// document has, or a type no document ever has (`[EXPEDIENTE_1]`).
  final List<String> invented;

  /// Id of another document the reply fits clearly better.
  final String? betterMatch;
}
