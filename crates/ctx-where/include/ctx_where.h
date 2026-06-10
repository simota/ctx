/* ctx-where — Phase 3 where port FFI surface. AUTO-GENERATED. */

#ifndef CTX_WHERE_H
#define CTX_WHERE_H

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
 * # Safety
 * See module-level docs.
 */
int ctx_where_search(const uint8_t *files_ptr,
                     uintptr_t files_len,
                     const uint8_t *query_ptr,
                     uintptr_t query_len,
                     const uint8_t *opts_ptr,
                     uintptr_t opts_len,
                     char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_where_suggest(const uint8_t *files_ptr,
                      uintptr_t files_len,
                      const uint8_t *query_ptr,
                      uintptr_t query_len,
                      int limit,
                      char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_where_levenshtein(const uint8_t *a_ptr,
                          uintptr_t a_len,
                          const uint8_t *b_ptr,
                          uintptr_t b_len,
                          int *out_dist);

/**
 * # Safety
 * See module-level docs.
 */
void ctx_where_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated version banner.
 */
const char *ctx_where_version(void);

/**
 * # Safety
 * `out_handle` must be a valid, writable pointer to a `*mut c_void`.
 * On success the caller owns the handle and must release it via
 * `ctx_where_session_close`.
 */
int ctx_where_session_open(const uint8_t *files_ptr,
                           uintptr_t files_len,
                           const uint8_t *opts_ptr,
                           uintptr_t opts_len,
                           void **out_handle);

/**
 * # Safety
 * `handle` must have been returned by a prior successful call to
 * `ctx_where_session_open` and must not have been passed to
 * `ctx_where_session_close`. `out_result_ptr` must be a valid writable
 * pointer to a `*mut c_char`.
 */
int ctx_where_session_search(void *handle,
                             const uint8_t *query_ptr,
                             uintptr_t query_len,
                             int limit,
                             char **out_result_ptr);

/**
 * # Safety
 * `handle` must either be null (no-op, returns -1) or a pointer
 * returned by `ctx_where_session_open` that has not previously been
 * passed to this function. Calling on a null handle is safe and
 * returns ERR_NULL_PTR; calling twice on the same non-null handle is
 * UNDEFINED — the caller must enforce single-close discipline.
 */
int ctx_where_session_close(void *handle);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_WHERE_H */
