/* ctx-scan — Phase 1 scan port FFI surface. AUTO-GENERATED. */

#ifndef CTX_SCAN_H
#define CTX_SCAN_H

/* Do not edit — regenerate via cargo build. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Scan `path` under `opts_json`. On success writes a JSON array of
 * Warning structs into `*out_result_ptr` (always an array, possibly
 * empty `[]`).
 *
 * # Safety
 * See module-level docs.
 */
int ctx_scan_file(const uint8_t *path_ptr,
                  uintptr_t path_len,
                  const uint8_t *opts_json_ptr,
                  uintptr_t opts_json_len,
                  char **out_result_ptr);

/**
 * Scan `text` (already-in-memory bytes) under `opts_json` as if it had
 * been read from `virtual_path`. Useful for callers that hold the
 * content in memory and don't want the disk round-trip (e.g. an
 * in-memory diff body).
 *
 * # Safety
 * See module-level docs.
 */
int ctx_scan_text(const uint8_t *text_ptr,
                  uintptr_t text_len,
                  const uint8_t *virtual_path_ptr,
                  uintptr_t virtual_path_len,
                  const uint8_t *opts_json_ptr,
                  uintptr_t opts_json_len,
                  char **out_result_ptr);

/**
 * Scan a batch of paths. `paths_json` is a JSON array of UTF-8
 * strings. Errors on individual paths are silently dropped (matching
 * the Go `continue` behaviour in `ScanFilesWithOptions`).
 *
 * # Safety
 * See module-level docs.
 */
int ctx_scan_files(const uint8_t *paths_json_ptr,
                   uintptr_t paths_json_len,
                   const uint8_t *opts_json_ptr,
                   uintptr_t opts_json_len,
                   char **out_result_ptr);

/**
 * Free a string previously returned from one of the `ctx_scan_*`
 * functions via `out_*_ptr`. Safe to call on a null pointer (no-op).
 *
 * # Safety
 * `s` must either be null or a pointer originally returned by this
 * crate's FFI. Calling on any other pointer is undefined behaviour.
 */
void ctx_scan_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated C string carrying
 * the crate's version banner. The caller MUST NOT free it.
 */
const char *ctx_scan_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_SCAN_H */
