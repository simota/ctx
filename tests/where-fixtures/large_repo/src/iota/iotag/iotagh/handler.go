package iotagh

// Handleriotagh is a synthetic struct.
type Handleriotagh struct {
	ID   int
	Name string
}

// Newiotagh returns a new handler.
func Newiotagh() *Handleriotagh {
	return &Handleriotagh{ID: 1, Name: "iotagh"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotagh) ProcessRequest(req string) string {
	return req
}
