package iotaig

// Handleriotaig is a synthetic struct.
type Handleriotaig struct {
	ID   int
	Name string
}

// Newiotaig returns a new handler.
func Newiotaig() *Handleriotaig {
	return &Handleriotaig{ID: 1, Name: "iotaig"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaig) ProcessRequest(req string) string {
	return req
}
