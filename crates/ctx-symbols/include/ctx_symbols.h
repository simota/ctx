/* ctx-symbols — Phase 4 Tier 2 #5 symbols pure-compute port FFI surface. AUTO-GENERATED. */

#ifndef CTX_SYMBOLS_H
#define CTX_SYMBOLS_H

/* Do not edit — regenerate via cargo build. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stddef.h>
#include <stdint.h>

#define ERR_OK 0

#define ERR_NULL_PTR -1

#define ERR_TOO_LARGE -2

#define ERR_BAD_JSON -3

#define ERR_SERIALIZE -4

#define ERR_BAD_HANDLE -5

#define ERR_BAD_KIND -6

#define ERR_PANIC -99

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * # Safety
 * `req_ptr` must be valid for `req_len` bytes. `out_result_ptr` must be
 * a valid, writable pointer to `*mut c_char`. On success the caller
 * owns the returned C string and MUST free via
 * `ctx_symbols_free_string`.
 */
int ctx_symbols_apionly_render(const uint8_t *req_ptr, uintptr_t req_len, char **out_result_ptr);

/**
 * # Safety
 * See `ctx_symbols_apionly_render` for the slice/out-ptr contract.
 */
int ctx_symbols_lookup_resolve(const uint8_t *corpus_ptr,
                               uintptr_t corpus_len,
                               const uint8_t *args_ptr,
                               uintptr_t args_len,
                               char **out_result_ptr);

/**
 * # Safety
 * `out_handle` must be a valid, writable pointer to `*mut c_void`.
 * On success the caller owns the handle and must release via
 * `ctx_symbols_lookup_session_close`.
 */
int ctx_symbols_lookup_session_open(const uint8_t *root_ptr,
                                    uintptr_t root_len,
                                    const uint8_t *corpus_ptr,
                                    uintptr_t corpus_len,
                                    void **out_handle);

/**
 * # Safety
 * `handle` must be a session pointer returned by `_session_open` and
 * not yet closed.
 */
int ctx_symbols_lookup_session_query(void *handle,
                                     const uint8_t *kind_ptr,
                                     uintptr_t kind_len,
                                     const uint8_t *args_ptr,
                                     uintptr_t args_len,
                                     char **out_result_ptr);

/**
 * # Safety
 * `handle` must either be null (returns ERR_NULL_PTR) or a pointer
 * returned by `_session_open` not yet passed to this function.
 */
int ctx_symbols_lookup_session_close(void *handle);

/**
 * # Safety
 * `s` must either be null (no-op) or a pointer returned by a prior
 * successful FFI call.
 */
void ctx_symbols_free_string(char *s);

/**
 * Returns a pointer to a `'static` NUL-terminated version banner.
 */
const char *ctx_symbols_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_SYMBOLS_H */
