# Context Pack

Goal: rate limiting behavior

## File contents

### pkg/rate/limit.go

```go
package rate

// Limiter enforces a rate limit with a burst allowance.
type Limiter struct {
	burst int
	rate  float64
}

func NewLimiter(rate float64, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

// AllowBurst reports whether n events fit in the burst budget.
func (l *Limiter) AllowBurst(n int) bool {
	return n <= l.burst
}
```

### pkg/server/handler.go

```go
package server

// HandleRequest serves an incoming request.
func HandleRequest() {}

func parseQuery(q string) string { return q }
```
