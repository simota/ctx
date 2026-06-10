package epsilonga

// Handlerepsilonga is a synthetic struct.
type Handlerepsilonga struct {
	ID   int
	Name string
}

// Newepsilonga returns a new handler.
func Newepsilonga() *Handlerepsilonga {
	return &Handlerepsilonga{ID: 1, Name: "epsilonga"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonga) ProcessRequest(req string) string {
	return req
}
