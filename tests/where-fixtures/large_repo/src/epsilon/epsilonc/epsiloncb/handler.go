package epsiloncb

// Handlerepsiloncb is a synthetic struct.
type Handlerepsiloncb struct {
	ID   int
	Name string
}

// Newepsiloncb returns a new handler.
func Newepsiloncb() *Handlerepsiloncb {
	return &Handlerepsiloncb{ID: 1, Name: "epsiloncb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsiloncb) ProcessRequest(req string) string {
	return req
}
