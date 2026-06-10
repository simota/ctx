package pack

import "strings"

// Relevance ranks files based on multiple heuristics.
type Relevance struct {
	Score int
}

// ScoreRelevance computes a score for the given path.
func ScoreRelevance(path string) Relevance {
	r := Relevance{}
	if strings.Contains(path, "pack") {
		r.Score = 10
	}
	return r
}
