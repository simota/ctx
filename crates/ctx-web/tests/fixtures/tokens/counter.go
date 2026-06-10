// Package tokens estimates token counts for source files.
package tokens

import (
	"os"
	"sync"

	tiktoken "github.com/pkoukk/tiktoken-go"
)

// Counter estimates token counts.
type Counter interface {
	CountString(text string) int
	CountFile(path string) (int, error)
}

// TiktokenCounter uses the cl100k_base encoding (GPT-4 / Claude compatible).
type TiktokenCounter struct {
	enc *tiktoken.Tiktoken
}

// sharedEncoder caches the cl100k_base BPE table. Loading it eagerly costs
// ~10–50 ms (decodes ~100k merge entries) — calling NewTiktokenCounter() once
// per HTTP request, file, or pack-included file used to repeat that work
// each time. The cl100k_base table is read-only, so concurrent Encode() calls
// against a shared *Tiktoken are safe.
var (
	sharedEncoderOnce sync.Once
	sharedEncoder     *tiktoken.Tiktoken
	sharedEncoderErr  error
)

func getSharedEncoder() (*tiktoken.Tiktoken, error) {
	sharedEncoderOnce.Do(func() {
		sharedEncoder, sharedEncoderErr = tiktoken.GetEncoding("cl100k_base")
	})
	return sharedEncoder, sharedEncoderErr
}

// NewTiktokenCounter constructs a counter using the cl100k_base encoding.
// The underlying BPE table is loaded once per process and shared across
// counter instances — this keeps the public API unchanged while eliminating
// the per-call decode cost on hot paths (HTTP handlers, pack builds).
func NewTiktokenCounter() (*TiktokenCounter, error) {
	enc, err := getSharedEncoder()
	if err != nil {
		return nil, err
	}
	return &TiktokenCounter{enc: enc}, nil
}

// CountString returns the token count for the given text.
func (c *TiktokenCounter) CountString(text string) int {
	return len(c.enc.Encode(text, nil, nil))
}

// CountFile reads the file at path and returns its token count.
func (c *TiktokenCounter) CountFile(path string) (int, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return 0, err
	}
	return c.CountString(string(data)), nil
}

// EstimateBySize returns a rough token estimate based on byte size (4 bytes/token heuristic).
func EstimateBySize(size int64) int {
	return int(size / 4)
}
