// scan-gen produces byte-identical benchmark fixtures shared by the
// Rust (criterion) and Go (testing.B) bench harnesses for ctx-scan.
//
// Output files land under tests/bench-inputs/ at the repo root. The
// generator is fully deterministic: every byte is derived from a
// fixed math/rand seed so re-running gen overwrites the files with
// the same content, guaranteeing the two language harnesses see
// byte-identical inputs.
//
// Run from the repo root:
//
//	go run ./tests/bench-inputs/scan-gen
//
// Layout produced:
//
//	tests/bench-inputs/
//	  scan_small.txt    ~2 KB body, ~5 real secret lines embedded
//	  scan_medium.txt   ~20 KB body, ~50 secret lines embedded
//	  scan_large.txt    ~200 KB body, ~500 secret lines embedded
//
// Each fixture interleaves "harmless filler" lines (English words) with
// realistic secret-shaped tokens at a fixed cadence so the scanner's
// regex hot path is exercised across the full body, not just at the
// edges.
package main

import (
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strings"
)

func main() {
	root := "tests/bench-inputs"
	if err := os.MkdirAll(root, 0o755); err != nil {
		fail("mkdir: %v", err)
	}
	specs := []struct {
		name        string
		path        string
		secretLines int
		fillerWords int
	}{
		{"small", filepath.Join(root, "scan_small.txt"), 5, 200},
		{"medium", filepath.Join(root, "scan_medium.txt"), 50, 2000},
		{"large", filepath.Join(root, "scan_large.txt"), 500, 20000},
	}
	for _, s := range specs {
		body := buildBody(s.secretLines, s.fillerWords, int64(len(s.name)))
		if err := os.WriteFile(s.path, []byte(body), 0o644); err != nil {
			fail("write %s: %v", s.path, err)
		}
		fmt.Printf("ok\t%s\t(%d bytes)\n", s.path, len(body))
	}
}

// buildBody interleaves filler and secret lines.  The seed is derived
// from the fixture name length so every fixture is reproducible but
// distinct (avoids the "all three fixtures are the same prefix"
// shortcut Criterion would otherwise reward).
func buildBody(secretLines, fillerWords int, seed int64) string {
	rng := rand.New(rand.NewSource(0xC75 + seed*31))

	words := []string{
		"the", "scan", "engine", "regex", "fixture", "benchmark",
		"hot", "path", "rust", "go", "criterion", "deterministic",
		"throughput", "secret", "allowlist", "entropy", "pattern",
	}
	secretGens := []func(*rand.Rand) string{
		genAwsKey,
		genGcpKey,
		genGithubPat,
		genSlackToken,
		genJwt,
		genEnvAssignment,
		genFiller, // ~1/7 of "secret" lines are decoys to mix the workload
	}

	var b strings.Builder
	b.Grow(fillerWords*8 + secretLines*64)

	// Distribute secrets uniformly across the filler. Every (fillerWords
	// / secretLines) words we splice a secret line.
	cadence := 1
	if secretLines > 0 {
		cadence = fillerWords / secretLines
		if cadence < 1 {
			cadence = 1
		}
	}
	wordsThisLine := 0
	wordsTotal := 0
	secretsEmitted := 0
	for wordsTotal < fillerWords {
		w := words[rng.Intn(len(words))]
		b.WriteString(w)
		b.WriteByte(' ')
		wordsThisLine++
		wordsTotal++
		if wordsThisLine >= 6+rng.Intn(6) {
			b.WriteByte('\n')
			wordsThisLine = 0
			if wordsTotal%cadence == 0 && secretsEmitted < secretLines {
				gen := secretGens[rng.Intn(len(secretGens))]
				b.WriteString(gen(rng))
				b.WriteByte('\n')
				secretsEmitted++
			}
		}
	}
	// Drain any remaining secrets at the tail so the fixture meets the
	// `secretLines` target exactly.
	for secretsEmitted < secretLines {
		gen := secretGens[rng.Intn(len(secretGens))]
		b.WriteString(gen(rng))
		b.WriteByte('\n')
		secretsEmitted++
	}
	return b.String()
}

func genAwsKey(r *rand.Rand) string {
	return fmt.Sprintf(`AWS_ACCESS_KEY_ID=example_secret_%s`, upperAlnum(r, 16))
}
func genGcpKey(r *rand.Rand) string {
	return fmt.Sprintf(`GCP_API_KEY=example_secret_%s`, mixedAlnum(r, 35))
}
func genGithubPat(r *rand.Rand) string {
	return fmt.Sprintf(`github_token="example_secret_%s"`, mixedAlnum(r, 36))
}
func genSlackToken(r *rand.Rand) string {
	return fmt.Sprintf(`slack_token="example_secret_%d_%s"`, 1000000000+r.Intn(900000000), mixedAlnum(r, 16))
}
func genJwt(r *rand.Rand) string {
	return fmt.Sprintf(`token="eyJ%s.eyJ%s.%s"`,
		mixedAlnum(r, 40), mixedAlnum(r, 60), mixedAlnum(r, 40))
}
func genEnvAssignment(r *rand.Rand) string {
	return fmt.Sprintf(`OPENAI_API_KEY=example_secret_%s`, mixedAlnum(r, 24))
}
func genFiller(r *rand.Rand) string {
	// A decoy line that looks vaguely like a secret but doesn't match
	// any regex — exercises the "try every pattern, fail fast" path.
	return fmt.Sprintf(`comment = "looks like %s but isn't"`, mixedAlnum(r, 18))
}

func upperAlnum(r *rand.Rand, n int) string {
	const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
	b := make([]byte, n)
	for i := range b {
		b[i] = chars[r.Intn(len(chars))]
	}
	return string(b)
}

func mixedAlnum(r *rand.Rand, n int) string {
	const chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
	b := make([]byte, n)
	for i := range b {
		b[i] = chars[r.Intn(len(chars))]
	}
	return string(b)
}

func fail(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "scan-gen: "+format+"\n", args...)
	os.Exit(1)
}
