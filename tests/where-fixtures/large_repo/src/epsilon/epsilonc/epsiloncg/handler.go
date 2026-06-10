package epsiloncg

// Handlerepsiloncg is a synthetic struct.
type Handlerepsiloncg struct {
	ID   int
	Name string
}

// Newepsiloncg returns a new handler.
func Newepsiloncg() *Handlerepsiloncg {
	return &Handlerepsiloncg{ID: 1, Name: "epsiloncg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsiloncg) ProcessRequest(req string) string {
	return req
}
