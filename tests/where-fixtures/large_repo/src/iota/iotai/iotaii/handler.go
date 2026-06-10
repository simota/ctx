package iotaii

// Handleriotaii is a synthetic struct.
type Handleriotaii struct {
	ID   int
	Name string
}

// Newiotaii returns a new handler.
func Newiotaii() *Handleriotaii {
	return &Handleriotaii{ID: 1, Name: "iotaii"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaii) ProcessRequest(req string) string {
	return req
}
