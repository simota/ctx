/* ctx-replay — Phase 3 replay port FFI surface. AUTO-GENERATED. */

#ifndef CTX_REPLAY_H
#define CTX_REPLAY_H

/* Do not edit — regenerate via cargo build. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stddef.h>
#include <stdint.h>

/**
 * Mirrors the Go `SchemaVersion` constant.
 */
#define SCHEMA_VERSION 1

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Diff two manifests and return DiffSummary JSON.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_replay_diff(const uint8_t *base_ptr,
                    uintptr_t base_len,
                    const uint8_t *cur_ptr,
                    uintptr_t cur_len,
                    int strict,
                    char **out_result_ptr);

/**
 * Compute the selection diff and return SelectionSummary JSON.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_replay_selection_diff(const uint8_t *a_ptr,
                              uintptr_t a_len,
                              const uint8_t *b_ptr,
                              uintptr_t b_len,
                              const uint8_t *sort_by_ptr,
                              uintptr_t sort_by_len,
                              char **out_result_ptr);

/**
 * Parses a replay duration string and writes nanoseconds to `out_nanos`.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_replay_parse_duration(const uint8_t *s_ptr, uintptr_t s_len, int64_t *out_nanos);

/**
 * Free a string previously returned from one of the `ctx_replay_*`
 * functions. Safe to call on null (no-op).
 *
 * # Safety
 * See module-level docs.
 */
void ctx_replay_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated C string carrying
 * the crate's version banner. The caller MUST NOT free it.
 */
const char *ctx_replay_version(void);

/**
 * Open a replay session against `dir`. On success the handle is owned
 * by the caller and must be released via `ctx_replay_session_close`.
 *
 * # Safety
 * `out_handle` must be a valid, writable pointer to `*mut c_void`.
 */
int ctx_replay_session_open(const uint8_t *dir_ptr,
                            uintptr_t dir_len,
                            const uint8_t *_opts_ptr,
                            uintptr_t opts_len,
                            void **out_handle);

/**
 * Run a kind-tagged query against the cached snapshot session.
 *
 * # Safety
 * `handle` must have been returned by a prior successful call to
 * `ctx_replay_session_open` and must not have been passed to
 * `ctx_replay_session_close`.
 */
int ctx_replay_session_query(void *handle,
                             const uint8_t *kind_ptr,
                             uintptr_t kind_len,
                             const uint8_t *args_ptr,
                             uintptr_t args_len,
                             char **out_result_ptr);

/**
 * Close the session and free Rust-side memory.
 *
 * # Safety
 * `handle` must either be null (returns ERR_NULL_PTR) or a pointer
 * returned by `ctx_replay_session_open` that has not previously been
 * passed to this function. The caller MUST enforce single-close.
 */
int ctx_replay_session_close(void *handle);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_REPLAY_H */
