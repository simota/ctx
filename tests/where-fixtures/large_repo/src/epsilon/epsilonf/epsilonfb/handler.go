package epsilonfb

// Handlerepsilonfb is a synthetic struct.
type Handlerepsilonfb struct {
	ID   int
	Name string
}

// Newepsilonfb returns a new handler.
func Newepsilonfb() *Handlerepsilonfb {
	return &Handlerepsilonfb{ID: 1, Name: "epsilonfb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonfb) ProcessRequest(req string) string {
	return req
}
