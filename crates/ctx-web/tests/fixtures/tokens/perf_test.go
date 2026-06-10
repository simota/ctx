package tokens

import "testing"

// BenchmarkSharedEncoderReuse references the package-level grouped-var symbol
// `sharedEncoder` (declared in `var ( ... )` in counter.go). It exists in the
// fixture to lock the grouped-block extraction regression: with the
// under-extraction bug, `sharedEncoder` was dropped from counter.go's symbol
// set, so this file would NOT match counter.go (matched_symbols missing
// "sharedEncoder"). It mirrors the real internal/web/handlers_perf_test.go
// cross-file match the reviewer observed.
func BenchmarkSharedEncoderReuse(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = sharedEncoder
	}
}
