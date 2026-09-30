/* Copyright 2026 the Docudis contributors. Licensed under Apache-2.0. */

#include "docudis.h"

int main(void) {
  DocudisV1Buffer output = {0};
  const char request[] =
      "{\"schema_version\":1,\"text\":\"Alice\",\"detections\":[{"
      "\"type\":\"PERSON\",\"value\":\"Alice\",\"start\":0,\"end\":5,"
      "\"confidence\":1.0,\"detector\":\"c-smoke\",\"source\":\"manual\","
      "\"enabled\":true}]}";
  if (docudis_v1_abi_version() != DOCUDIS_V1_ABI_VERSION) return 1;
  if (docudis_v1_version() == NULL) return 2;
  if (docudis_v1_anonymize_json((const uint8_t *)request,
                                sizeof(request) - 1,
                                &output) != DOCUDIS_V1_OK) {
    return 3;
  }
  if (output.ptr == NULL || output.len == 0) return 4;
  docudis_v1_buffer_free(&output);
  if (output.ptr != NULL || output.len != 0 || output.capacity != 0) return 5;

  const char detect[] =
      "{\"schema_version\":1,\"text\":\"Alice at alice@example.com\"," 
      "\"regions\":[],\"dictionary\":[\"Alice\"],\"detections\":[]}";
  if (docudis_v1_detect_json((const uint8_t *)detect, sizeof(detect) - 1,
                             &output) != DOCUDIS_V1_OK) return 6;
  if (output.ptr == NULL || output.len == 0) return 7;
  docudis_v1_buffer_free(&output);
  if (docudis_v1_process_json((const uint8_t *)detect, sizeof(detect) - 1,
                              &output) != DOCUDIS_V1_OK) return 8;
  if (output.ptr == NULL || output.len == 0) return 9;
  docudis_v1_buffer_free(&output);

  const char restore[] =
      "{\"schema_version\":1,\"text\":\"Hello [PERSON_1]\","
      "\"mappings\":[{\"original\":\"Alice\","
      "\"placeholder\":\"[PERSON_1]\",\"type\":\"PERSON\"}]}";
  if (docudis_v1_restore_json((const uint8_t *)restore,
                              sizeof(restore) - 1,
                              &output) != DOCUDIS_V1_OK) return 10;
  if (output.ptr == NULL || output.len == 0) return 11;
  docudis_v1_buffer_free(&output);
  return 0;
}
