package iotajg

// Handleriotajg is a synthetic struct.
type Handleriotajg struct {
	ID   int
	Name string
}

// Newiotajg returns a new handler.
func Newiotajg() *Handleriotajg {
	return &Handleriotajg{ID: 1, Name: "iotajg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotajg) ProcessRequest(req string) string {
	return req
}
