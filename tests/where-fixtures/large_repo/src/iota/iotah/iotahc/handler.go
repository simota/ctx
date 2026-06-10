package iotahc

// Handleriotahc is a synthetic struct.
type Handleriotahc struct {
	ID   int
	Name string
}

// Newiotahc returns a new handler.
func Newiotahc() *Handleriotahc {
	return &Handleriotahc{ID: 1, Name: "iotahc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotahc) ProcessRequest(req string) string {
	return req
}
