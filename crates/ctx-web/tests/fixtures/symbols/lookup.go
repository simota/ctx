package symbols

import (
	"path"
	"path/filepath"
	"sort"
	"strings"
	"unicode"

	"github.com/simota/ctx/internal/model"
	"github.com/simota/ctx/internal/walk"
)

// Hit is one match returned by LookupByName. Path is forward-slash and
// repo-relative (relative to the root passed to LookupByName).
//
// Column is intentionally absent here because model.Symbol does not carry
// column information today; callers that need to advertise an optional
// column field at the wire layer can add it as nil/omitempty.
type Hit struct {
	Path       string
	Line       int
	Kind       string
	SymbolName string
}

// LookupOptions tunes LookupByName.
//
//   - From is an optional repo-relative anchor path (the file the user is
//     looking from). When set, hits in the same directory as From are
//     ranked first, then hits sharing From's first path segment. From may
//     point at a file that does not exist on disk; only its lexical parent
//     directory and first segment are used for tie-breaking.
//   - Kind, when non-empty, restricts results to symbols with the matching
//     kind string (e.g. "func", "method", "type", "class", "interface").
//     Kind aliases are accepted: "func" matches model.SymbolFunction.
type LookupOptions struct {
	From string
	Kind string
}

// LookupByName walks root (gitignore-aware) and returns every symbol whose
// name equals name, sorted by:
//
//  1. From's directory match (same directory as From wins)
//  2. From's first path segment match (e.g. "internal/...") wins
//  3. exported / public symbols win over non-exported (Go: leading uppercase)
//  4. lexical Path order
//
// When From is empty, steps 1 and 2 are skipped.
//
// Performance: the current implementation walks + extracts on each call
// (no cache). For repos under ~5k source files this stays under ~50ms on
// modern hardware. Cache layering can be added later without touching the
// signature.
func LookupByName(root, name string, opts LookupOptions) ([]Hit, error) {
	if name == "" {
		return nil, nil
	}

	walker, err := walk.New(root, walk.DefaultOptions())
	if err != nil {
		return nil, err
	}
	tree, err := walker.Walk(root)
	if err != nil {
		return nil, err
	}

	wantKind, kindFilter := normalizeKind(opts.Kind)
	extractor := New()

	var hits []Hit
	for _, fi := range walk.Flatten(tree) {
		if fi == nil || fi.IsDir {
			continue
		}
		syms, errE := extractor.Extract(fi.AbsPath)
		if errE != nil || len(syms) == 0 {
			continue
		}
		for _, s := range syms {
			if s.Name != name {
				continue
			}
			if kindFilter && string(s.Kind) != wantKind {
				continue
			}
			hits = append(hits, Hit{
				Path:       filepath.ToSlash(fi.Path),
				Line:       s.Line,
				Kind:       string(s.Kind),
				SymbolName: s.Name,
			})
		}
	}

	sortHits(hits, opts.From)
	return hits, nil
}

// sortHits applies the four-stage stable sort described on LookupByName.
// from may be empty; in that case only steps 3 and 4 apply.
func sortHits(hits []Hit, from string) {
	fromDir, fromSeg := anchorParts(from)
	sort.SliceStable(hits, func(i, j int) bool {
		hi, hj := hits[i], hits[j]

		if from != "" {
			si, sj := sameDir(hi.Path, fromDir), sameDir(hj.Path, fromDir)
			if si != sj {
				return si
			}
			fi, fj := sameFirstSegment(hi.Path, fromSeg), sameFirstSegment(hj.Path, fromSeg)
			if fi != fj {
				return fi
			}
		}

		pi, pj := isExported(hi.SymbolName), isExported(hj.SymbolName)
		if pi != pj {
			return pi
		}
		return hi.Path < hj.Path
	})
}

// anchorParts returns (dir, firstSegment) of a forward-slash path. Both
// fields are empty when from is empty. If from has no directory part
// ("foo.go"), dir is "" (root) and firstSegment is "foo.go".
func anchorParts(from string) (string, string) {
	if from == "" {
		return "", ""
	}
	clean := path.Clean(filepath.ToSlash(from))
	dir := path.Dir(clean)
	if dir == "." {
		dir = ""
	}
	first := clean
	if i := strings.IndexByte(clean, '/'); i >= 0 {
		first = clean[:i]
	}
	return dir, first
}

func sameDir(p, dir string) bool {
	d := path.Dir(p)
	if d == "." {
		d = ""
	}
	return d == dir
}

func sameFirstSegment(p, seg string) bool {
	if seg == "" {
		return false
	}
	if i := strings.IndexByte(p, '/'); i >= 0 {
		return p[:i] == seg
	}
	return p == seg
}

// isExported reports whether name starts with an uppercase letter, which
// in Go denotes an exported identifier. Other supported languages (TS, JS,
// Python) do not have a syntactic export marker on the symbol name itself,
// so this acts as a best-effort "public-looking" preference rather than a
// strict export check. Empty names sort last.
func isExported(name string) bool {
	if name == "" {
		return false
	}
	r := []rune(name)[0]
	return unicode.IsUpper(r)
}

// normalizeKind maps user-supplied kind aliases ("func", "fn") to the
// canonical model.SymbolKind string. Returns (canonical, true) when a
// filter should be applied, ("", false) when kind is empty.
func normalizeKind(kind string) (string, bool) {
	if kind == "" {
		return "", false
	}
	switch strings.ToLower(strings.TrimSpace(kind)) {
	case "func", "fn", "function":
		return string(model.SymbolFunction), true
	case "method":
		return string(model.SymbolMethod), true
	case "type":
		return string(model.SymbolType), true
	case "class":
		return string(model.SymbolClass), true
	case "interface":
		return string(model.SymbolInterface), true
	case "export":
		return string(model.SymbolExport), true
	default:
		return strings.ToLower(strings.TrimSpace(kind)), true
	}
}
