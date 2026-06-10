package iotaid

// Handleriotaid is a synthetic struct.
type Handleriotaid struct {
	ID   int
	Name string
}

// Newiotaid returns a new handler.
func Newiotaid() *Handleriotaid {
	return &Handleriotaid{ID: 1, Name: "iotaid"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaid) ProcessRequest(req string) string {
	return req
}
