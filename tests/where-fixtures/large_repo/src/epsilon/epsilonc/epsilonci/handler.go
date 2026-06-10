package epsilonci

// Handlerepsilonci is a synthetic struct.
type Handlerepsilonci struct {
	ID   int
	Name string
}

// Newepsilonci returns a new handler.
func Newepsilonci() *Handlerepsilonci {
	return &Handlerepsilonci{ID: 1, Name: "epsilonci"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonci) ProcessRequest(req string) string {
	return req
}
