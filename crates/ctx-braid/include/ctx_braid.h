/* ctx-braid — Phase 4 Tier 2 #1 braid pure-compute port FFI surface. AUTO-GENERATED. */

#ifndef CTX_BRAID_H
#define CTX_BRAID_H

/* Do not edit — regenerate via cargo build. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stddef.h>
#include <stdint.h>

/**
 * SchemaVersion is the current braid.toml schema. Bump when the layout
 * changes in a non-additive way. Mirrors Go's `SchemaVersion = 1`.
 */
#define SCHEMA_VERSION 1

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * # Safety
 * `toml_ptr` must be valid for `toml_len` bytes (zero length permitted
 * with NULL pointer). `out_result_ptr` must be a valid writable
 * pointer to a `*mut c_char`.
 */
int ctx_braid_load_config(const uint8_t *toml_ptr, uintptr_t toml_len, char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_braid_validate(const uint8_t *cfg_json_ptr, uintptr_t cfg_json_len, char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_braid_allocate(const uint8_t *cfg_json_ptr,
                       uintptr_t cfg_json_len,
                       int64_t global_budget,
                       char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_braid_merge_paths(const uint8_t *sels_json_ptr,
                          uintptr_t sels_json_len,
                          char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_braid_shell_quote(const uint8_t *src_ptr, uintptr_t src_len, char **out_result_ptr);

/**
 * # Safety
 * See module-level docs.
 */
int ctx_braid_strand_subcommand(const uint8_t *src_ptr, uintptr_t src_len, char **out_result_ptr);

/**
 * # Safety
 * `s` must either be null (no-op) or a pointer returned by a prior
 * successful FFI call.
 */
void ctx_braid_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated version banner.
 */
const char *ctx_braid_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_BRAID_H */
