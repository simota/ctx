# Context Pack

**Goal**: rate limit burst handler
**Generated**: 2026-05-29T10:00:00Z
**Budget**: 600 / 50000 tokens

## Included files (3 files, 600 tokens)

### High relevance
- middleware/limit.go (250 tokens)
- middleware/limit_test.go (200 tokens)
- docs/intro.md (150 tokens)

---

## File contents

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	// burst-aware limit check
	return true
}

// BurstHandler is a request-time hook for burst-resolution.
func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestBurst(t *testing.T) {
	l := NewLimiter(10, 5)
	// verify burst behaviour
	if !l.Allow() {
		t.Fatal("burst limit failed")
	}
}

func TestBurstHandler(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### docs/intro.md

```markdown
This document describes the project goals and high-level architecture.
There is nothing about rate limiting here, just general prose about
overall service design and deployment topology.
```
