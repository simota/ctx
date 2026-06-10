package iotaeb

// Handleriotaeb is a synthetic struct.
type Handleriotaeb struct {
	ID   int
	Name string
}

// Newiotaeb returns a new handler.
func Newiotaeb() *Handleriotaeb {
	return &Handleriotaeb{ID: 1, Name: "iotaeb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaeb) ProcessRequest(req string) string {
	return req
}
