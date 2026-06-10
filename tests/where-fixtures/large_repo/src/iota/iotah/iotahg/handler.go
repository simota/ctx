package iotahg

// Handleriotahg is a synthetic struct.
type Handleriotahg struct {
	ID   int
	Name string
}

// Newiotahg returns a new handler.
func Newiotahg() *Handleriotahg {
	return &Handleriotahg{ID: 1, Name: "iotahg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotahg) ProcessRequest(req string) string {
	return req
}
