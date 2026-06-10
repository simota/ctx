# Context Pack

**Goal**: rate limit burst handler
**Generated**: 2026-05-29T10:00:00Z

## Included files

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

### docs/intro.md

```go
Documentation about the system architecture. Mostly prose with
no specific rate-limiting content here.
```

### docs/limit.md

```go
Documentation about limit. Includes design rationale and
configuration guidance.
```

### docs/handler.md

```go
Documentation about handler. Includes design rationale and
configuration guidance.
```

### pkg/util/strings.go

```go
package util

// strings utilities. Generic helpers used across services.
func Helperstrings() string {
	return ""
}
```

### pkg/util/numbers.go

```go
package util

// numbers utilities. Generic helpers used across services.
func Helpernumbers() string {
	return ""
}
```

### middleware/limit.go

```go
package middleware

// Limiter enforces rate limit with burst tolerance.
type Limiter struct {
	rate  int
	burst int
	last  int64
}

func NewLimiter(rate, burst int) *Limiter {
	return &Limiter{rate: rate, burst: burst}
}

func (l *Limiter) Allow() bool {
	return true
}

func (l *Limiter) BurstHandler(req int) bool {
	return req <= l.burst
}
```

### middleware/burst.go

```go
package middleware

// BurstController tracks burst capacity across a request window.
type BurstController struct {
	cap   int
	used  int
}

func NewBurstController(cap int) *BurstController {
	return &BurstController{cap: cap}
}

func (b *BurstController) Consume(n int) bool {
	if b.used+n > b.cap {
		return false
	}
	b.used += n
	return true
}

func (b *BurstController) Reset() {
	b.used = 0
}
```

### middleware/handler.go

```go
package middleware

import "net/http"

// RateLimitHandler wraps http.Handler with burst-aware throttling.
type RateLimitHandler struct {
	inner   http.Handler
	limiter *Limiter
}

func NewRateLimitHandler(h http.Handler, l *Limiter) *RateLimitHandler {
	return &RateLimitHandler{inner: h, limiter: l}
}

func (r *RateLimitHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if !r.limiter.Allow() {
		http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	r.inner.ServeHTTP(w, req)
}
```

### middleware/limit_test.go

```go
package middleware

import "testing"

func TestLimit(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.Allow() {
		t.Fatal("limit failed")
	}
}

func TestBurstHandlerAcceptsSmall(t *testing.T) {
	l := NewLimiter(10, 5)
	if !l.BurstHandler(3) {
		t.Fatal("burst handler should accept 3 when burst=5")
	}
}
```

### middleware/burst_test.go

```go
package middleware

import "testing"

func TestBurstController(t *testing.T) {
	b := NewBurstController(10)
	if !b.Consume(3) {
		t.Fatal("Consume should succeed under cap")
	}
	if b.Consume(8) {
		t.Fatal("Consume should fail above cap")
	}
}
```

### internal/rate/rate.go

```go
package rate

// Rate represents a per-second token rate.
type Rate float64

func (r Rate) Per(d float64) float64 {
	return float64(r) * d
}
```

### internal/rate/sliding_window.go

```go
package rate

import "time"

// SlidingWindow implements a sliding-window rate limit.
type SlidingWindow struct {
	window time.Duration
	count  int
	last   time.Time
}

func (s *SlidingWindow) Allow() bool {
	now := time.Now()
	if now.Sub(s.last) > s.window {
		s.count = 0
		s.last = now
	}
	s.count++
	return true
}
```

### internal/rate/token_bucket.go

```go
package rate

// TokenBucket implements a classic token-bucket limiter with burst capacity.
type TokenBucket struct {
	tokens int
	cap    int
	rate   int
}

func (t *TokenBucket) Take(n int) bool {
	if t.tokens < n {
		return false
	}
	t.tokens -= n
	return true
}

func (t *TokenBucket) Refill(amount int) {
	t.tokens += amount
	if t.tokens > t.cap {
		t.tokens = t.cap
	}
}
```

### internal/audit/log.go

```go
package audit

import "time"

// Entry is one audit log row.
type Entry struct {
	When   time.Time
	Actor  string
	Action string
}

func Record(entry Entry) error {
	return nil
}
```

### internal/cache/lru.go

```go
package cache

// LRU is a least-recently-used cache.
type LRU struct {
	capacity int
	items    map[string]int
}

func NewLRU(cap int) *LRU {
	return &LRU{capacity: cap, items: map[string]int{}}
}
```

### internal/cache/lfu.go

```go
package cache

// LFU is a least-frequently-used cache.
type LFU struct {
	capacity int
	freq     map[string]int
}

func NewLFU(cap int) *LFU {
	return &LFU{capacity: cap, freq: map[string]int{}}
}
```

