/* Copyright 2026 the Docudis contributors. Licensed under Apache-2.0. */

#ifndef DOCUDIS_H
#define DOCUDIS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#  if defined(DOCUDIS_BUILD_SHARED)
#    define DOCUDIS_API __declspec(dllexport)
#  else
#    define DOCUDIS_API
#  endif
#else
#  define DOCUDIS_API __attribute__((visibility("default")))
#endif

#define DOCUDIS_V1_ABI_VERSION 1u

typedef enum DocudisV1Status {
  DOCUDIS_V1_OK = 0,
  DOCUDIS_V1_INVALID_ARGUMENT = 1,
  DOCUDIS_V1_INVALID_UTF8 = 2,
  DOCUDIS_V1_INVALID_JSON = 3,
  DOCUDIS_V1_CORE_ERROR = 4,
  DOCUDIS_V1_PANIC = 255
} DocudisV1Status;

/* Owned output bytes. Treat fields as read-only and release with
 * docudis_v1_buffer_free on the same Docudis library that allocated them. */
typedef struct DocudisV1Buffer {
  uint8_t *ptr;
  size_t len;
  size_t capacity;
} DocudisV1Buffer;

DOCUDIS_API uint32_t docudis_v1_abi_version(void);

/* Static UTF-8/NUL-terminated library version. Never free this pointer. */
DOCUDIS_API const char *docudis_v1_version(void);

/*
 * Input and output are UTF-8 JSON using schema_version 1. Detection start/end
 * fields and response replacement start/end fields are half-open UTF-8 byte
 * offsets. On success, `out` owns a UTF-8 JSON buffer (not NUL-terminated).
 *
 * Input schema:
 *   {"schema_version":1,"text":"...","detections":[...],
 *    "previous_map":[{"original":"...","placeholder":"[PERSON_1]",
 *                     "type":"PERSON"}]}
 *
 * The caller retains ownership of `input`. The caller must initialize and
 * pass a writable `out`; on failure it is reset to an empty buffer.
 */
DOCUDIS_API DocudisV1Status docudis_v1_anonymize_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Runs bundled regex rules and optional dictionary/list matching, then merges
 * caller-provided detections (including Dart-produced NER spans). `regions`
 * contains lower-case pack names. All offsets are UTF-8 bytes.
 *
 * Input schema:
 *   {"schema_version":1,"text":"...","regions":["fr"],
 *    "dictionary":["..."],"never_hide":["..."],
 *    "include_bundled_lists":false,"detections":[...]}
 * Output: {"schema_version":1,"detections":[...]}
 */
DOCUDIS_API DocudisV1Status docudis_v1_detect_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Same request as docudis_v1_detect_json, with optional `previous_map`.
 * Returns the schema-v1 anonymization response plus the final merged
 * `detections` array, so a host can differentially compare the pipeline. */
DOCUDIS_API DocudisV1Status docudis_v1_process_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Input: {"schema_version":1,"text":"[PERSON_1]", "mappings":[...]}
 * Output: {"schema_version":1,"text":"Alice"} */
DOCUDIS_API DocudisV1Status docudis_v1_restore_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Cuts text into the review page's one-tap chunks. `taken` holds the spans
 * shown as placeholders; no chunk crosses one. All offsets are UTF-8 bytes.
 *
 * Input:  {"schema_version":1,"text":"...","taken":[{"start":0,"end":5}]}
 * Output: {"schema_version":1,"chunks":[{"start":6,"end":9}]} */
DOCUDIS_API DocudisV1Status docudis_v1_chunk_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Resolves overlaps and propagates enabled values among the given
 * detections without detecting anything new (review edits).
 *
 * Input:  {"schema_version":1,"text":"...","detections":[...]}
 * Output: {"schema_version":1,"detections":[...]} */
DOCUDIS_API DocudisV1Status docudis_v1_merge_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Rule-pack regions for a text's detected BCP-47 language tags; Han text
 * outside Japanese also turns on "cn". `regions` is sorted.
 *
 * Input:  {"schema_version":1,"languages":["en-GB"],"text":"..."}
 * Output: {"schema_version":1,"regions":["gb","ie","us"]} */
DOCUDIS_API DocudisV1Status docudis_v1_regions_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* ISO 639-1 tags of the languages plausibly present in `text`, most likely
 * first, for hosts without language identification of their own. Fails
 * with DOCUDIS_V1_CORE_ERROR when the library was built without the
 * `language-id` feature.
 *
 * Input:  {"schema_version":1,"text":"..."}
 * Output: {"schema_version":1,"languages":["fr"]} */
DOCUDIS_API DocudisV1Status docudis_v1_languages_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Checks whether a pasted AI reply answers document `id` among
 * `candidates` (each record's anonymized text and its placeholders).
 * `better_match` is null or the id of a document the reply fits clearly
 * better.
 *
 * Input:  {"schema_version":1,"reply":"...","id":"a",
 *          "candidates":[{"id":"a","text":"...","placeholders":["[PERSON_1]"]}]}
 * Output: {"schema_version":1,"unknown":["[EMAIL_1]"],
 *          "invented":["[PERSON_4]"],"better_match":null} */
DOCUDIS_API DocudisV1Status docudis_v1_reply_check_json(
    const uint8_t *input,
    size_t input_len,
    DocudisV1Buffer *out);

/* Releases a successful output buffer and zeroes it. NULL is accepted. */
DOCUDIS_API void docudis_v1_buffer_free(DocudisV1Buffer *buffer);

/* Thread-local, static, NUL-terminated detail for the most recent failure on
 * the current thread. Valid until the next C API call on that thread. Never
 * free this pointer. The empty string means there is no current error. */
DOCUDIS_API const char *docudis_v1_last_error_message(void);

#ifdef __cplusplus
}
#endif

#endif /* DOCUDIS_H */
