// gen produces byte-identical benchmark fixtures shared by the Rust
// (criterion) and Go (testing.B) bench harnesses for ctx-contract.
//
// Output files land under tests/bench-inputs/ at the repo root. The
// generator is fully deterministic: every byte is derived from a fixed
// math/rand seed so re-running gen overwrites the files with the same
// content, guaranteeing the two language harnesses see byte-identical
// inputs.
//
// Run from the repo root:
//
//	go run ./tests/bench-inputs/gen
//
// Layout produced:
//
//	tests/bench-inputs/
//	  extract_small.txt           ~10 refs over ~2 KB prose
//	  extract_medium.txt          ~100 refs over ~20 KB prose
//	  extract_large.txt           ~1000 refs over ~200 KB prose
//	  verify_contract.json        Contract with 50 files
//	  verify_response.txt         response citing ~30 of those files
//	  parse_md.txt                ~500 KB pack body with markdown contract block
//	  parse_json.json             ~500 KB pack body with JSON-form contract block
package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strings"
)

// Reproduce the contract.Contract shape inline so this generator stays
// dependency-free (no import cycle with internal/contract while we live
// next to it).
type benchFile struct {
	Path      string   `json:"path"`
	LineStart int      `json:"line_start"`
	LineEnd   int      `json:"line_end"`
	SHA256    string   `json:"sha256"`
	Symbols   []string `json:"symbols,omitempty"`
}

type benchContract struct {
	SchemaVersion int         `json:"schema_version"`
	Created       string      `json:"created"`
	Files         []benchFile `json:"files"`
}

func main() {
	root := repoRoot()
	out := filepath.Join(root, "tests", "bench-inputs")
	if err := os.MkdirAll(out, 0o755); err != nil {
		die(err)
	}

	// ---- ExtractReferences inputs ------------------------------------
	for _, c := range []struct {
		name  string
		refs  int
		prose int // prose bytes between refs
	}{
		{"extract_small.txt", 10, 200},
		{"extract_medium.txt", 100, 200},
		{"extract_large.txt", 1000, 200},
	} {
		body := buildExtractCorpus(c.refs, c.prose)
		writeFile(filepath.Join(out, c.name), body)
	}

	// ---- Verify inputs ----------------------------------------------
	contract, response := buildVerifyFixtures(50, 30)
	cjson, err := json.MarshalIndent(contract, "", "  ")
	if err != nil {
		die(err)
	}
	writeFile(filepath.Join(out, "verify_contract.json"), cjson)
	writeFile(filepath.Join(out, "verify_response.txt"), response)

	// ---- ParseFromPack inputs ---------------------------------------
	mdBody := buildPackBodyWithMarkdownContract(50, 500*1024)
	writeFile(filepath.Join(out, "parse_md.txt"), mdBody)

	jsonBody := buildPackBodyWithJSONContract(50, 500*1024)
	writeFile(filepath.Join(out, "parse_json.json"), jsonBody)

	fmt.Printf("wrote bench fixtures to %s\n", out)
}

// buildExtractCorpus generates `nRefs` reference citations of mixed kind
// (file / line-range / symbol / diff-header) interleaved with `prose`
// bytes of filler between each one. The result is a UTF-8 text buffer
// suitable for ExtractReferences.
func buildExtractCorpus(nRefs, prose int) []byte {
	rng := rand.New(rand.NewSource(0xC0FFEE))
	exts := []string{".go", ".ts", ".tsx", ".py", ".rs", ".md", ".json", ".yaml", ".sh"}
	dirs := []string{
		"internal/contract", "internal/pack", "internal/limit",
		"cmd/ctx", "web/src/pages", "crates/ctx-contract/src",
		"docs/specs", "tests/parity",
	}
	syms := []string{
		"ExtractReferences", "ParseFromPack", "Verify", "Build",
		"sha256Hex", "lookupPath", "rangeContained", "lastDotSegment",
		"renderJSON", "renderMarkdown",
	}

	var b strings.Builder
	for i := 0; i < nRefs; i++ {
		// Prose filler — random-looking but deterministic.
		b.WriteString(fillerLines(rng, prose))

		kind := i % 4
		switch kind {
		case 0:
			// plain file ref
			fmt.Fprintf(&b, "see %s/%s%s for the implementation.\n",
				dirs[rng.Intn(len(dirs))],
				randIdent(rng, 6),
				exts[rng.Intn(len(exts))])
		case 1:
			// line-range with GitHub-style L prefix
			start := rng.Intn(400) + 1
			end := start + rng.Intn(30)
			fmt.Fprintf(&b, "look at %s/%s%s:L%d-L%d for context.\n",
				dirs[rng.Intn(len(dirs))],
				randIdent(rng, 6),
				exts[rng.Intn(len(exts))],
				start, end)
		case 2:
			// symbol reference
			fmt.Fprintf(&b, "call `%s` before returning.\n", syms[rng.Intn(len(syms))])
		case 3:
			// diff header
			fmt.Fprintf(&b, "+++ b/%s/%s%s\n",
				dirs[rng.Intn(len(dirs))],
				randIdent(rng, 6),
				exts[rng.Intn(len(exts))])
		}
	}
	return []byte(b.String())
}

// buildVerifyFixtures returns a Contract with `nFiles` files and a
// response that references `nCited` of them in mixed-kind form. The
// contract's Created timestamp is frozen so both harnesses share it.
func buildVerifyFixtures(nFiles, nCited int) (benchContract, []byte) {
	rng := rand.New(rand.NewSource(0xBEEF))
	dirs := []string{
		"internal/contract", "internal/pack", "internal/limit",
		"cmd/ctx", "web/src/pages",
	}
	exts := []string{".go", ".ts", ".py", ".rs", ".md"}
	allSyms := []string{
		"Build", "Verify", "ExtractReferences", "ParseFromPack",
		"Format", "Embed", "StripContractBlock", "ToJSONField",
		"rangeContained", "lookupPath",
	}

	c := benchContract{
		SchemaVersion: 1,
		Created:       "2026-05-29T00:00:00Z",
		Files:         make([]benchFile, 0, nFiles),
	}
	paths := make([]string, 0, nFiles)
	for i := 0; i < nFiles; i++ {
		p := fmt.Sprintf("%s/file_%03d%s",
			dirs[i%len(dirs)],
			i,
			exts[i%len(exts)])
		paths = append(paths, p)
		body := []byte(fillerLines(rng, 4096))
		sum := sha256.Sum256(body)
		// pick 1-3 symbols
		ns := 1 + rng.Intn(3)
		syms := make([]string, 0, ns)
		for j := 0; j < ns; j++ {
			syms = append(syms, allSyms[rng.Intn(len(allSyms))])
		}
		c.Files = append(c.Files, benchFile{
			Path:      p,
			LineStart: 1,
			LineEnd:   200,
			SHA256:    hex.EncodeToString(sum[:]),
			Symbols:   dedupSorted(syms),
		})
	}

	// Build a response citing nCited of the files plus a few phantom
	// (out-of-context) refs and phantom symbols to exercise both
	// happy-path and violation paths.
	var resp strings.Builder
	for i := 0; i < nCited; i++ {
		p := paths[i]
		switch i % 3 {
		case 0:
			fmt.Fprintf(&resp, "look at %s for the call site.\n", p)
		case 1:
			fmt.Fprintf(&resp, "%s:L10-L40 contains the logic.\n", p)
		case 2:
			fmt.Fprintf(&resp, "+++ b/%s\n", p)
		}
		// Sprinkle a couple of symbol refs.
		fmt.Fprintf(&resp, "Use `%s` and `%s` to wire it.\n",
			c.Files[i].Symbols[0], allSyms[(i*7)%len(allSyms)])
	}
	// A few phantom paths.
	for i := 0; i < 5; i++ {
		fmt.Fprintf(&resp, "see imaginary/phantom_%02d.go\n", i)
	}
	// A few phantom symbols.
	for i := 0; i < 5; i++ {
		fmt.Fprintf(&resp, "call `Ghost%d` first\n", i)
	}
	return c, []byte(resp.String())
}

// buildPackBodyWithMarkdownContract produces a pack-body-shaped corpus
// containing a markdown HTML-comment contract block somewhere inside.
// `totalSize` is the approximate total byte size; the contract block is
// inserted near the middle so ParseFromPack must scan past prose to find
// it.
func buildPackBodyWithMarkdownContract(nFiles, totalSize int) []byte {
	c, _ := buildVerifyFixtures(nFiles, 1)
	body, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		die(err)
	}
	block := fmt.Sprintf("\n<!-- ctx:contract v1\n%s\n-->\n", string(body))
	rng := rand.New(rand.NewSource(0xFACE))
	prose := fillerLines(rng, totalSize-len(block))
	mid := len(prose) / 2
	return []byte(prose[:mid] + block + prose[mid:])
}

// buildPackBodyWithJSONContract wraps the contract under a top-level
// "contract" key inside a JSON object so ParseFromPack hits the JSON
// probe branch.
func buildPackBodyWithJSONContract(nFiles, totalSize int) []byte {
	c, _ := buildVerifyFixtures(nFiles, 1)
	// Add filler payload so the resulting JSON is ~totalSize.
	rng := rand.New(rand.NewSource(0xBADD))
	pad := fillerLines(rng, totalSize)
	doc := map[string]interface{}{
		"pack":     "synthetic-bench",
		"payload":  pad,
		"contract": c,
	}
	out, err := json.Marshal(doc)
	if err != nil {
		die(err)
	}
	return out
}

func fillerLines(rng *rand.Rand, approxBytes int) string {
	if approxBytes <= 0 {
		return ""
	}
	words := []string{
		"the", "contract", "references", "pack", "manifest", "verify",
		"sha256", "rust", "go", "regex", "scanner", "context",
		"benchmark", "deterministic", "fixture", "symbol", "path",
	}
	var b strings.Builder
	b.Grow(approxBytes + 64)
	for b.Len() < approxBytes {
		ll := 4 + rng.Intn(8)
		for j := 0; j < ll; j++ {
			b.WriteString(words[rng.Intn(len(words))])
			if j < ll-1 {
				b.WriteByte(' ')
			}
		}
		b.WriteByte('\n')
	}
	return b.String()
}

func randIdent(rng *rand.Rand, n int) string {
	const letters = "abcdefghijklmnopqrstuvwxyz"
	b := make([]byte, n)
	for i := range b {
		b[i] = letters[rng.Intn(len(letters))]
	}
	return string(b)
}

func dedupSorted(in []string) []string {
	seen := map[string]struct{}{}
	out := make([]string, 0, len(in))
	for _, s := range in {
		if _, ok := seen[s]; ok {
			continue
		}
		seen[s] = struct{}{}
		out = append(out, s)
	}
	return out
}

func writeFile(path string, body []byte) {
	if err := os.WriteFile(path, body, 0o644); err != nil {
		die(err)
	}
	fmt.Printf("  %s (%d bytes)\n", path, len(body))
}

func repoRoot() string {
	// gen is invoked from anywhere; walk up looking for go.mod.
	cwd, err := os.Getwd()
	if err != nil {
		die(err)
	}
	dir := cwd
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			die(fmt.Errorf("go.mod not found above %s", cwd))
		}
		dir = parent
	}
}

func die(err error) {
	fmt.Fprintln(os.Stderr, "gen:", err)
	os.Exit(1)
}
