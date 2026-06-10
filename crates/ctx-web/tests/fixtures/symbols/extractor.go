// Package symbols extracts named code constructs from source files.
package symbols

import (
	"context"
	"os"
	"path/filepath"
	"strings"

	sitter "github.com/smacker/go-tree-sitter"
	"github.com/smacker/go-tree-sitter/golang"
	"github.com/smacker/go-tree-sitter/javascript"
	"github.com/smacker/go-tree-sitter/python"
	"github.com/smacker/go-tree-sitter/typescript/tsx"
	"github.com/smacker/go-tree-sitter/typescript/typescript"

	"github.com/simota/ctx/internal/model"
)

const maxParseBytes = 500 * 1024

// Extractor defines the interface for symbol extraction backends.
type Extractor interface {
	Extract(path string) ([]model.Symbol, error)
}

// TreeSitterExtractor extracts symbols using tree-sitter language grammars.
type TreeSitterExtractor struct{}

// New returns the default Extractor.
func New() Extractor {
	return TreeSitterExtractor{}
}

type langSpec struct {
	language *sitter.Language
	kinds    map[string]model.SymbolKind
}

var langSpecs = map[string]langSpec{
	".go": {
		language: golang.GetLanguage(),
		kinds: map[string]model.SymbolKind{
			"function_declaration": model.SymbolFunction,
			"method_declaration":   model.SymbolMethod,
			"type_spec":            model.SymbolType,
		},
	},
	".ts": {
		language: typescript.GetLanguage(),
		kinds: map[string]model.SymbolKind{
			"function_declaration":   model.SymbolFunction,
			"class_declaration":      model.SymbolClass,
			"interface_declaration":  model.SymbolInterface,
			"type_alias_declaration": model.SymbolType,
		},
	},
	".tsx": {
		language: tsx.GetLanguage(),
		kinds: map[string]model.SymbolKind{
			"function_declaration":   model.SymbolFunction,
			"class_declaration":      model.SymbolClass,
			"interface_declaration":  model.SymbolInterface,
			"type_alias_declaration": model.SymbolType,
		},
	},
	".js": {
		language: javascript.GetLanguage(),
		kinds: map[string]model.SymbolKind{
			"function_declaration": model.SymbolFunction,
			"class_declaration":    model.SymbolClass,
		},
	},
	".jsx": {
		language: javascript.GetLanguage(),
		kinds: map[string]model.SymbolKind{
			"function_declaration": model.SymbolFunction,
			"class_declaration":    model.SymbolClass,
		},
	},
	".mjs": {
		language: javascript.GetLanguage(),
		kinds: map[string]model.SymbolKind{
			"function_declaration": model.SymbolFunction,
			"class_declaration":    model.SymbolClass,
		},
	},
	".py": {
		language: python.GetLanguage(),
		kinds: map[string]model.SymbolKind{
			"function_definition": model.SymbolFunction,
			"class_definition":    model.SymbolClass,
		},
	},
}

// Extract returns named symbols for supported source files.
func (TreeSitterExtractor) Extract(path string) ([]model.Symbol, error) {
	info, err := os.Stat(path)
	if err != nil {
		return nil, err
	}
	if info.Size() > maxParseBytes {
		return nil, nil
	}

	spec, ok := langSpecs[strings.ToLower(filepath.Ext(path))]
	if !ok {
		return nil, nil
	}

	source, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	parser := sitter.NewParser()
	parser.SetLanguage(spec.language)
	tree, err := parser.ParseCtx(context.Background(), nil, source)
	if err != nil {
		return nil, nil
	}
	defer tree.Close()

	var out []model.Symbol
	seen := make(map[string]bool)
	walkNode(tree.RootNode(), source, spec.kinds, &out, seen)
	return out, nil
}

func walkNode(n *sitter.Node, source []byte, kinds map[string]model.SymbolKind, out *[]model.Symbol, seen map[string]bool) {
	if n == nil {
		return
	}
	if kind, ok := kinds[n.Type()]; ok {
		nameNode := n.ChildByFieldName("name")
		if nameNode != nil {
			name := nameNode.Content(source)
			key := string(kind) + "\x00" + name
			if !seen[key] {
				seen[key] = true
				*out = append(*out, model.Symbol{
					Name: name,
					Kind: kind,
					Line: int(n.StartPoint().Row) + 1,
				})
			}
		}
	}
	count := int(n.NamedChildCount())
	for i := 0; i < count; i++ {
		walkNode(n.NamedChild(i), source, kinds, out, seen)
	}
}
