/* ctx-heatmap — Phase 4 heatmap port FFI surface. AUTO-GENERATED. */

#ifndef CTX_HEATMAP_H
#define CTX_HEATMAP_H

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
 * `metrics_ptr`/`opts_ptr` must be valid for `metrics_len`/`opts_len`
 * bytes (zero length permitted with NULL pointers).
 * `out_result_ptr` must be a valid writable pointer to a `*mut c_char`.
 */
int ctx_heatmap_aggregate(const uint8_t *metrics_ptr,
                          uintptr_t metrics_len,
                          const uint8_t *opts_ptr,
                          uintptr_t opts_len,
                          char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_heatmap_squarify(const uint8_t *buckets_ptr,
                         uintptr_t buckets_len,
                         int w,
                         int h,
                         char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_heatmap_render_ascii(const uint8_t *rects_ptr,
                             uintptr_t rects_len,
                             const uint8_t *opts_ptr,
                             uintptr_t opts_len,
                             char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_heatmap_render_json(const uint8_t *rects_ptr,
                            uintptr_t rects_len,
                            const uint8_t *opts_ptr,
                            uintptr_t opts_len,
                            char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_heatmap_render_plain(const uint8_t *buckets_ptr,
                             uintptr_t buckets_len,
                             const uint8_t *opts_ptr,
                             uintptr_t opts_len,
                             char **out_result_ptr);

/**
 * # Safety
 * `s` must either be null (no-op) or a pointer returned by a prior
 * successful FFI call.
 */
void ctx_heatmap_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated version banner.
 */
const char *ctx_heatmap_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_HEATMAP_H */
