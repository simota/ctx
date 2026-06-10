/* ctx-focus — Phase 4 focus port FFI surface. AUTO-GENERATED. */

#ifndef CTX_FOCUS_H
#define CTX_FOCUS_H

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
int ctx_focus_pack(const uint8_t *files_ptr,
                   uintptr_t files_len,
                   const uint8_t *anchor_ptr,
                   uintptr_t anchor_len,
                   int hops,
                   char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
void ctx_focus_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated version banner.
 */
const char *ctx_focus_version(void);

/**
 * # Safety
 * `out_handle` must be a valid, writable pointer to a `*mut c_void`.
 * On success the caller owns the handle and must release it via
 * `ctx_focus_session_close`.
 */
int ctx_focus_session_open(const uint8_t *files_ptr,
                           uintptr_t files_len,
                           const uint8_t *opts_ptr,
                           uintptr_t opts_len,
                           void **out_handle);

/**
 * # Safety
 * `handle` must have been returned by a prior successful call to
 * `ctx_focus_session_open` and must not have been passed to
 * `ctx_focus_session_close`.
 */
int ctx_focus_session_resolve(void *handle,
                              const uint8_t *anchor_ptr,
                              uintptr_t anchor_len,
                              char **out_result_ptr);

/**
 * # Safety
 * `handle` must have been returned by a prior successful call to
 * `ctx_focus_session_open` and must not have been passed to
 * `ctx_focus_session_close`.
 */
int ctx_focus_session_expand(void *handle,
                             const uint8_t *anchor_ptr,
                             uintptr_t anchor_len,
                             int hops,
                             char **out_result_ptr);

/**
 * # Safety
 * See ctx_focus_session_expand.
 */
int ctx_focus_session_pack(void *handle,
                           const uint8_t *anchor_ptr,
                           uintptr_t anchor_len,
                           int hops,
                           char **out_result_ptr);

/**
 * # Safety
 * `handle` must either be null (returns ERR_NULL_PTR) or a pointer
 * returned by `ctx_focus_session_open` that has not previously been
 * passed to this function. The caller MUST enforce single-close.
 */
int ctx_focus_session_close(void *handle);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_FOCUS_H */
