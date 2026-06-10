package epsilonig

// Handlerepsilonig is a synthetic struct.
type Handlerepsilonig struct {
	ID   int
	Name string
}

// Newepsilonig returns a new handler.
func Newepsilonig() *Handlerepsilonig {
	return &Handlerepsilonig{ID: 1, Name: "epsilonig"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonig) ProcessRequest(req string) string {
	return req
}
