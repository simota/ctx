package iotach

// Handleriotach is a synthetic struct.
type Handleriotach struct {
	ID   int
	Name string
}

// Newiotach returns a new handler.
func Newiotach() *Handleriotach {
	return &Handleriotach{ID: 1, Name: "iotach"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotach) ProcessRequest(req string) string {
	return req
}
