package tokens

import (
	"sync"
	"testing"
)

// TestNewTiktokenCounterSharedInstance verifies that two calls to
// NewTiktokenCounter return counters that share the same underlying encoder
// object — the sync.Once singleton contract.
func TestNewTiktokenCounterSharedInstance(t *testing.T) {
	c1, err := NewTiktokenCounter()
	if err != nil {
		t.Fatalf("first NewTiktokenCounter: %v", err)
	}
	c2, err := NewTiktokenCounter()
	if err != nil {
		t.Fatalf("second NewTiktokenCounter: %v", err)
	}
	// Both counters must point to the same *Tiktoken object.
	if c1.enc != c2.enc {
		t.Error("sharedEncoder singleton broken: c1.enc != c2.enc")
	}
}

// TestCountStringDeterministic verifies that repeated calls to CountString on
// the same text always return the same token count (cl100k_base table is read-only).
func TestCountStringDeterministic(t *testing.T) {
	c, err := NewTiktokenCounter()
	if err != nil {
		t.Fatalf("NewTiktokenCounter: %v", err)
	}
	const text = "hello world, this is a regression test for the shared encoder"
	want := c.CountString(text)
	if want == 0 {
		t.Fatal("CountString returned 0 for non-empty input")
	}
	for i := 0; i < 10; i++ {
		if got := c.CountString(text); got != want {
			t.Errorf("iter %d: got %d want %d", i, got, want)
		}
	}
}

// TestConcurrentEncodeRaceFree spawns 500 goroutines each creating a counter
// and calling CountString 20 times. The test is designed to be run with
// -race; any data race on the shared encoder will be detected.
func TestConcurrentEncodeRaceFree(t *testing.T) {
	const goroutines = 500
	const iterations = 20
	const text = "the quick brown fox jumps over the lazy dog"

	// Compute the expected count once before the concurrent section.
	ref, err := NewTiktokenCounter()
	if err != nil {
		t.Fatalf("reference counter: %v", err)
	}
	want := ref.CountString(text)

	var wg sync.WaitGroup
	errs := make(chan string, goroutines*iterations)

	for g := 0; g < goroutines; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			c, err := NewTiktokenCounter()
			if err != nil {
				errs <- "NewTiktokenCounter: " + err.Error()
				return
			}
			for i := 0; i < iterations; i++ {
				if got := c.CountString(text); got != want {
					errs <- "wrong count"
					return
				}
			}
		}()
	}

	wg.Wait()
	close(errs)

	for e := range errs {
		t.Error(e)
	}
}

// TestConcurrentNewTiktokenCounter exercises concurrent construction to ensure
// the sync.Once path does not introduce a race when multiple goroutines race
// to initialise the encoder simultaneously.
func TestConcurrentNewTiktokenCounter(t *testing.T) {
	const goroutines = 100
	var wg sync.WaitGroup
	errs := make(chan error, goroutines)

	for g := 0; g < goroutines; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, err := NewTiktokenCounter()
			if err != nil {
				errs <- err
			}
		}()
	}

	wg.Wait()
	close(errs)

	for err := range errs {
		t.Errorf("concurrent NewTiktokenCounter: %v", err)
	}
}
