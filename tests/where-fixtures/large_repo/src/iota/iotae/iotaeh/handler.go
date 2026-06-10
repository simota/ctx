package iotaeh

// Handleriotaeh is a synthetic struct.
type Handleriotaeh struct {
	ID   int
	Name string
}

// Newiotaeh returns a new handler.
func Newiotaeh() *Handleriotaeh {
	return &Handleriotaeh{ID: 1, Name: "iotaeh"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaeh) ProcessRequest(req string) string {
	return req
}
