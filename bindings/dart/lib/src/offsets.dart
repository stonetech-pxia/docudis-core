// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'dart:convert';

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
