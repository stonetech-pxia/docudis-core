// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'dart:convert';
import 'dart:typed_data';

int utf16ToUtf8Offset(String text, int offset) {
  if (offset < 0 || offset > text.length) {
    throw RangeError.range(offset, 0, text.length, 'offset');
  }
  if (offset > 0 && offset < text.length) {
    final previous = text.codeUnitAt(offset - 1);
    final next = text.codeUnitAt(offset);
    if (previous >= 0xD800 &&
        previous <= 0xDBFF &&
        next >= 0xDC00 &&
        next <= 0xDFFF) {
      throw RangeError('UTF-16 offset splits a surrogate pair');
    }
  }
  return utf8.encode(text.substring(0, offset)).length;
}

int utf8ToUtf16Offset(String text, int offset) {
  final bytes = utf8.encode(text);
  if (offset < 0 || offset > bytes.length) {
    throw RangeError.range(offset, 0, bytes.length, 'offset');
  }
  try {
    return utf8.decode(bytes.sublist(0, offset)).length;
  } on FormatException {
    throw RangeError('UTF-8 offset is not a character boundary');
  }
}

Map<String, Object?> detectionUtf16ToUtf8(
  String text,
  Map<String, Object?> detection,
) => {
  ...detection,
  'start': utf16ToUtf8Offset(text, detection['start']! as int),
  'end': utf16ToUtf8Offset(text, detection['end']! as int),
};

Map<String, Object?> detectionUtf8ToUtf16(
  String text,
  Map<String, Object?> detection,
) => {
  ...detection,
  'start': utf8ToUtf16Offset(text, detection['start']! as int),
  'end': utf8ToUtf16Offset(text, detection['end']! as int),
};

/// Converts many offsets into one text in both directions after indexing it
/// once, with the same checks as [utf16ToUtf8Offset] and
/// [utf8ToUtf16Offset].
class OffsetIndex {
  OffsetIndex(this.text) {
    final utf8At = Int32List(text.length + 1);
    final utf16At = <int, int>{0: 0};
    var bytes = 0;
    for (var i = 0; i < text.length; i++) {
      final unit = text.codeUnitAt(i);
      final pair =
          unit >= 0xD800 &&
          unit <= 0xDBFF &&
          i + 1 < text.length &&
          (text.codeUnitAt(i + 1) & 0xFC00) == 0xDC00;
      utf8At[i] = bytes;
      if (pair) {
        utf8At[i + 1] = -1; // inside a surrogate pair
        bytes += 4;
        i++;
      } else {
        bytes += unit < 0x80
            ? 1
            : unit < 0x800
            ? 2
            : 3;
      }
      utf16At[bytes] = i + 1;
    }
    utf8At[text.length] = bytes;
    _utf8At = utf8At;
    _utf16At = utf16At;
    utf8Length = bytes;
  }

  final String text;
  late final Int32List _utf8At;
  late final Map<int, int> _utf16At;
  late final int utf8Length;

  int toUtf8(int offset) {
    if (offset < 0 || offset > text.length) {
      throw RangeError.range(offset, 0, text.length, 'offset');
    }
    final bytes = _utf8At[offset];
    if (bytes < 0) throw RangeError('UTF-16 offset splits a surrogate pair');
    return bytes;
  }

  int toUtf16(int offset) {
    if (offset < 0 || offset > utf8Length) {
      throw RangeError.range(offset, 0, utf8Length, 'offset');
    }
    return _utf16At[offset] ??
        (throw RangeError('UTF-8 offset is not a character boundary'));
  }
}
