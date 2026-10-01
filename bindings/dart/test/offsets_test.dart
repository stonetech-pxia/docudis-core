// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

import 'package:docudis_ffi/docudis_ffi.dart';
import 'package:test/test.dart';

void main() {
  test('converts ASCII, CJK, emoji, and combining offsets both ways', () {
    const text = 'A张😀e\u0301Z';
    const boundaries = [
      (0, 0),
      (1, 1),
      (2, 4),
      (4, 8),
      (5, 9),
      (6, 11),
      (7, 12),
    ];
    for (final (utf16, utf8) in boundaries) {
      expect(utf16ToUtf8Offset(text, utf16), utf8);
      expect(utf8ToUtf16Offset(text, utf8), utf16);
    }
    expect(() => utf16ToUtf8Offset(text, 3), throwsRangeError);
    expect(() => utf8ToUtf16Offset(text, 6), throwsRangeError);
  });

  test('OffsetIndex agrees with the one-off conversions', () {
    const text = 'A张😀e\u0301Z\uD800x 𠀀';
    final index = OffsetIndex(text);
    for (var i = 0; i <= text.length; i++) {
      final expected = _attempt(() => utf16ToUtf8Offset(text, i));
      expect(_attempt(() => index.toUtf8(i)), expected, reason: 'utf16 $i');
    }
    for (var i = 0; i <= index.utf8Length + 1; i++) {
      final expected = _attempt(() => utf8ToUtf16Offset(text, i));
      expect(_attempt(() => index.toUtf16(i)), expected, reason: 'utf8 $i');
    }
  });
}

/// The offset, or null when the conversion rejects it.
int? _attempt(int Function() convert) {
  try {
    return convert();
  } on RangeError {
    return null;
  }
}
