package iotadg

// Handleriotadg is a synthetic struct.
type Handleriotadg struct {
	ID   int
	Name string
}

// Newiotadg returns a new handler.
func Newiotadg() *Handleriotadg {
	return &Handleriotadg{ID: 1, Name: "iotadg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadg) ProcessRequest(req string) string {
	return req
}
