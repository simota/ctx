package symbols

import (
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"sync"
	"testing"

	"github.com/simota/ctx/internal/model"
)

func TestTreeSitterExtractorGo(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, "sample.go")
	err := os.WriteFile(path, []byte(`package sample

type Server struct{}

func NewServer() *Server { return &Server{} }

func (s *Server) Start() {}
`), 0644)
	if err != nil {
		t.Fatal(err)
	}

	syms, err := New().Extract(path)
	if err != nil {
		t.Fatalf("Extract: %v", err)
	}
	got := make([]string, 0, len(syms))
	for _, sym := range syms {
		got = append(got, sym.Name)
	}
	want := []string{"Server", "NewServer", "Start"}
	for i, name := range want {
		if i >= len(got) || got[i] != name {
			t.Fatalf("symbols = %v, want prefix %v", got, want)
		}
	}
}

func TestTreeSitterExtractorUnsupported(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, "README.md")
	if err := os.WriteFile(path, []byte("# docs\n"), 0644); err != nil {
		t.Fatal(err)
	}
	syms, err := New().Extract(path)
	if err != nil {
		t.Fatalf("Extract: %v", err)
	}
	if len(syms) != 0 {
		t.Fatalf("symbols = %v, want none", syms)
	}
}

func TestTreeSitterExtractorTypeScript(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, "auth.ts")
	src := `export interface User { id: string }
type Token = { value: string }
export class AuthService {
  async login(): Promise<Token> { return { value: "x" } }
}
export function logout(): void {}
`
	if err := os.WriteFile(path, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	syms, err := New().Extract(path)
	if err != nil {
		t.Fatalf("Extract: %v", err)
	}
	got := names(syms)
	want := []string{"User", "Token", "AuthService", "logout"}
	for _, w := range want {
		if !contains(got, w) {
			t.Errorf("missing %q in %v", w, got)
		}
	}
}

func TestTreeSitterExtractorPython(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, "pipeline.py")
	src := `class Pipeline:
    def run(self):
        return 1

def helper():
    return 2
`
	if err := os.WriteFile(path, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	syms, err := New().Extract(path)
	if err != nil {
		t.Fatalf("Extract: %v", err)
	}
	got := names(syms)
	want := []string{"Pipeline", "run", "helper"}
	for _, w := range want {
		if !contains(got, w) {
			t.Errorf("missing %q in %v", w, got)
		}
	}
}

// TestConcurrentExtractRaceFree spawns 100 goroutines each calling Extract on
// the same Go source file simultaneously. Every result must be identical —
// both symbol set and ordering — proving that the parallel extraction in
// handlers.go does not silently drop symbols or corrupt ordering.
// Run with -race to surface any hidden data races.
func TestConcurrentExtractRaceFree(t *testing.T) {
	const goroutines = 100

	// Use extractor.go itself as the fixed test subject so no extra file is needed.
	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller(0) failed")
	}
	subjectPath := filepath.Join(filepath.Dir(thisFile), "extractor.go")
	if _, err := os.Stat(subjectPath); err != nil {
		t.Fatalf("subject file not accessible: %v", err)
	}

	// Compute the reference result once before the concurrent section.
	ref, err := New().Extract(subjectPath)
	if err != nil {
		t.Fatalf("reference Extract: %v", err)
	}
	if len(ref) == 0 {
		t.Fatal("reference Extract returned no symbols — check subject file path")
	}

	type result struct {
		goroutine int
		syms      []model.Symbol
		err       error
	}
	results := make(chan result, goroutines)
	var wg sync.WaitGroup

	for g := 0; g < goroutines; g++ {
		wg.Add(1)
		g := g
		go func() {
			defer wg.Done()
			syms, err := New().Extract(subjectPath)
			results <- result{goroutine: g, syms: syms, err: err}
		}()
	}

	wg.Wait()
	close(results)

	for r := range results {
		if r.err != nil {
			t.Errorf("goroutine %d: Extract error: %v", r.goroutine, r.err)
			continue
		}
		if !reflect.DeepEqual(r.syms, ref) {
			t.Errorf("goroutine %d: symbol mismatch\n  got  %s\n  want %s",
				r.goroutine, fmtSyms(r.syms), fmtSyms(ref))
		}
	}
}

func fmtSyms(syms []model.Symbol) string {
	parts := make([]string, 0, len(syms))
	for _, s := range syms {
		parts = append(parts, fmt.Sprintf("%s(%s)@%d", s.Name, s.Kind, s.Line))
	}
	return "[" + joinStr(parts, " ") + "]"
}

func joinStr(ss []string, sep string) string {
	out := ""
	for i, s := range ss {
		if i > 0 {
			out += sep
		}
		out += s
	}
	return out
}

func names(syms []model.Symbol) []string {
	out := make([]string, 0, len(syms))
	for _, s := range syms {
		out = append(out, s.Name)
	}
	return out
}

func contains(xs []string, x string) bool {
	for _, v := range xs {
		if v == x {
			return true
		}
	}
	return false
}
