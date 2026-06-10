package iotajc

// Handleriotajc is a synthetic struct.
type Handleriotajc struct {
	ID   int
	Name string
}

// Newiotajc returns a new handler.
func Newiotajc() *Handleriotajc {
	return &Handleriotajc{ID: 1, Name: "iotajc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotajc) ProcessRequest(req string) string {
	return req
}
