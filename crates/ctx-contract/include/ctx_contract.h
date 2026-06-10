/* ctx-contract — Summit pioneer FFI surface. AUTO-GENERATED. */

#ifndef CTX_CONTRACT_H
#define CTX_CONTRACT_H

/* Do not edit — regenerate via cargo build. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stddef.h>
#include <stdint.h>

/**
 * SchemaVersion is the current contract manifest schema.
 *
 * Mirrors `internal/contract/contract.go`'s `SchemaVersion = 1`.
 */
#define SCHEMA_VERSION 1

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Verify `response` against `contract_json` under `opts_json`. On
 * success writes a JSON-encoded `Result` into `*out_result_ptr`.
 *
 * `opts_json` may be a null pointer + len=0 to use defaults.
 *
 * # Safety
 * All input pointers must either be null (len 0) or point to a valid
 * initialised buffer of at least the indicated length. `out_result_ptr`
 * must point to writable storage for one `*mut c_char`.
 */
int ctx_contract_verify(const uint8_t *contract_json_ptr,
                        uintptr_t contract_json_len,
                        const uint8_t *response_ptr,
                        uintptr_t response_len,
                        const uint8_t *opts_json_ptr,
                        uintptr_t opts_json_len,
                        char **out_result_ptr);

/**
 * Extract every reference from `response`. Writes a JSON array
 * (possibly empty) into `*out_refs_ptr`.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_contract_extract_references(const uint8_t *response_ptr,
                                    uintptr_t response_len,
                                    char **out_refs_ptr);

/**
 * Search `pack` for an embedded contract block. On success writes the
 * contract JSON into `*out_contract_ptr` and sets `*out_found = 1`. If
 * no contract is embedded, `*out_found = 0`, `*out_contract_ptr` is
 * null, and the function still returns `ERR_OK`.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_contract_parse_from_pack(const uint8_t *pack_ptr,
                                 uintptr_t pack_len,
                                 char **out_contract_ptr,
                                 int *out_found);

/**
 * Strip the embedded contract block from `pack`. Writes a raw byte
 * buffer (NOT null-terminated) into `*out_stripped_ptr` with the byte
 * count in `*out_len`. When `*out_len == 0`, `*out_stripped_ptr` is
 * null and no free is required.
 *
 * # Safety
 * See module-level docs.
 */
int ctx_contract_strip_block(const uint8_t *pack_ptr,
                             uintptr_t pack_len,
                             uint8_t **out_stripped_ptr,
                             uintptr_t *out_len);

/**
 * Free a string previously returned from one of the `ctx_contract_*`
 * functions via `out_*_ptr`. Safe to call on a null pointer (no-op).
 *
 * # Safety
 * `s` must either be null or a pointer originally returned by this
 * crate's FFI. Calling on any other pointer is undefined behaviour.
 */
void ctx_contract_free_string(char *s);

/**
 * Free a byte buffer previously returned by `ctx_contract_strip_block`.
 * Safe to call when `buf` is null AND `len == 0`. Calling with
 * mismatched `len` is undefined behaviour.
 *
 * # Safety
 * `(buf, len)` must be the exact pair returned by a previous
 * `ctx_contract_strip_block` call.
 */
void ctx_contract_free_buffer(uint8_t *buf, uintptr_t len);

/**
 * Returns a pointer to a `'static` NUL-terminated C string carrying
 * the crate's version banner. The pointer is valid for the lifetime
 * of the loaded library; the caller MUST NOT free it.
 */
const char *ctx_contract_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* CTX_CONTRACT_H */
