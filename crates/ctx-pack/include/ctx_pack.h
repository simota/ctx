/* ctx-pack — Phase 4 Tier 2 #2 pack pure-compute port FFI surface. AUTO-GENERATED. */

#ifndef CTX_PACK_H
#define CTX_PACK_H

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
 * `goal_ptr` valid for `goal_len`; `out_handle` writable.
 */
int ctx_pack_relevance_session_open(const uint8_t *goal_ptr,
                                    uintptr_t goal_len,
                                    int64_t budget,
                                    void **out_handle);

/**
 * # Safety
 * `handle` must come from session_open and not yet be closed.
 */
int ctx_pack_relevance_session_score(void *handle,
                                     const uint8_t *file_json_ptr,
                                     uintptr_t file_json_len,
                                     int64_t token_count,
                                     char **out_result_ptr);

/**
 * Score every file in `files_json` against the session's goal and
 * return the result array (one entry per input file, in order).
 *
 * # Safety
 * See module-level docs.
 */
int ctx_pack_relevance_session_score_corpus(void *handle,
                                            const uint8_t *files_json_ptr,
                                            uintptr_t files_json_len,
                                            const uint8_t *tokens_json_ptr,
                                            uintptr_t tokens_json_len,
                                            char **out_result_ptr);

/**
 * Rank top-N files by relevance against the session's goal.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_pack_relevance_session_rank(void *handle,
                                    const uint8_t *files_json_ptr,
                                    uintptr_t files_json_len,
                                    const uint8_t *tokens_json_ptr,
                                    uintptr_t tokens_json_len,
                                    int n,
                                    char **out_result_ptr);

/**
 * Close a session and reclaim its memory.
 *
 * # Safety
 * `handle` must either be null or a value returned by
 * `ctx_pack_relevance_session_open` that has not been closed yet.
 */
int ctx_pack_relevance_session_close(void *handle);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_pack_relevance_score(const uint8_t *file_json_ptr,
                             uintptr_t file_json_len,
                             const uint8_t *goal_ptr,
                             uintptr_t goal_len,
                             int64_t token_count,
                             int64_t budget,
                             char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_pack_diff(const uint8_t *diffs_json_ptr,
                  uintptr_t diffs_json_len,
                  const uint8_t *opts_json_ptr,
                  uintptr_t opts_json_len,
                  char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_pack_redact(const uint8_t *data_ptr,
                    uintptr_t data_len,
                    const uint8_t *warnings_json_ptr,
                    uintptr_t warnings_json_len,
                    uint8_t **out_buf,
                    uintptr_t *out_len);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_pack_from_where(const uint8_t *data_ptr, uintptr_t data_len, char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_pack_preset(const uint8_t *name_ptr, uintptr_t name_len, char **out_result_ptr);

/**
 * # Safety
 * `s` must come from a prior ctx_pack_* call or be null.
 */
void ctx_pack_free_string(char *s);

/**
 * Free a buffer allocated by `ctx_pack_redact`.
 *
 * # Safety
 * `buf` must come from `ctx_pack_redact` or be null. `len` must match
 * the length returned by that call.
 */
void ctx_pack_free_bytes(uint8_t *buf, uintptr_t len);

const char *ctx_pack_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_PACK_H */
