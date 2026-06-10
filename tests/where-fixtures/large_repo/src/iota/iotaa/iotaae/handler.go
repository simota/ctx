package iotaae

// Handleriotaae is a synthetic struct.
type Handleriotaae struct {
	ID   int
	Name string
}

// Newiotaae returns a new handler.
func Newiotaae() *Handleriotaae {
	return &Handleriotaae{ID: 1, Name: "iotaae"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaae) ProcessRequest(req string) string {
	return req
}
