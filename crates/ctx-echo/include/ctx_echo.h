/* ctx-echo — Tier 2 #3 BM25 evaluator FFI surface. AUTO-GENERATED. */

#ifndef CTX_ECHO_H
#define CTX_ECHO_H

/* Do not edit — regenerate via cargo build. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stddef.h>
#include <stdint.h>

#define BM25_K1 1.5

#define BM25_B 0.75

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Run the BM25 evaluator. On success writes a JSON-encoded EchoResult
 * into `*out_result_ptr`. Empty `opts_json` (len=0) is treated as
 * `Options::default()`.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_echo_evaluate(const uint8_t *pack_path_ptr,
                      uintptr_t pack_path_len,
                      const uint8_t *pack_body_ptr,
                      uintptr_t pack_body_len,
                      const uint8_t *opts_json_ptr,
                      uintptr_t opts_json_len,
                      char **out_result_ptr);

/**
 * Free a string previously returned from one of the `ctx_echo_*`
 * functions via `out_*_ptr`. Safe to call on a null pointer (no-op).
 *
 * # Safety
 * `s` must either be null or a pointer originally returned by this
 * crate's FFI. Calling on any other pointer is undefined behaviour.
 */
void ctx_echo_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated C string carrying
 * the crate's version banner. The caller MUST NOT free it.
 */
const char *ctx_echo_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_ECHO_H */
