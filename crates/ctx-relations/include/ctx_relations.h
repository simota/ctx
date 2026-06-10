/* ctx-relations — Phase 2 relations port FFI surface. AUTO-GENERATED. */

#ifndef CTX_RELATIONS_H
#define CTX_RELATIONS_H

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
 * Build the relations Index for `root` and emit the JSON
 * serialization into `*out_result_ptr`.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_relations_build(const uint8_t *root_ptr, uintptr_t root_len, char **out_result_ptr);

/**
 * Build the relations Index for `root`, hitting the in-memory cache
 * if available. See `crate::cache::build_cached` for invalidation
 * semantics.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_relations_build_cached(const uint8_t *root_ptr, uintptr_t root_len, char **out_result_ptr);

/**
 * Drop any cached Index for `root`.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_relations_invalidate_cache(const uint8_t *root_ptr, uintptr_t root_len);

/**
 * Free a string previously returned from one of the
 * `ctx_relations_*` functions via `out_result_ptr`. Safe to call on a
 * null pointer (no-op).
 *
 * # Safety
 * `s` must either be null or a pointer originally returned by this
 * crate's FFI. Calling on any other pointer is undefined behaviour.
 */
void ctx_relations_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated C string carrying
 * the crate's version banner. The caller MUST NOT free it.
 */
const char *ctx_relations_version(void);

/**
 * # Safety
 * `out_handle` must be a valid, writable pointer to a `*mut c_void`.
 * On success the caller owns the handle and must release it via
 * `ctx_relations_session_close`.
 */
int ctx_relations_session_open(const uint8_t *root_ptr,
                               uintptr_t root_len,
                               const uint8_t *_opts_ptr,
                               uintptr_t opts_len,
                               void **out_handle);

/**
 * # Safety
 * `handle` must have been returned by a prior successful call to
 * `ctx_relations_session_open` and must not have been passed to
 * `ctx_relations_session_close`.
 */
int ctx_relations_session_query(void *handle,
                                const uint8_t *kind_ptr,
                                uintptr_t kind_len,
                                const uint8_t *args_ptr,
                                uintptr_t args_len,
                                char **out_result_ptr);

/**
 * # Safety
 * `handle` must either be null (returns ERR_NULL_PTR) or a pointer
 * returned by `ctx_relations_session_open` that has not previously been
 * passed to this function. The caller MUST enforce single-close.
 */
int ctx_relations_session_close(void *handle);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_RELATIONS_H */
