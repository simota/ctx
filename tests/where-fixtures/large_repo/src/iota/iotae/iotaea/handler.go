package iotaea

// Handleriotaea is a synthetic struct.
type Handleriotaea struct {
	ID   int
	Name string
}

// Newiotaea returns a new handler.
func Newiotaea() *Handleriotaea {
	return &Handleriotaea{ID: 1, Name: "iotaea"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaea) ProcessRequest(req string) string {
	return req
}
