package iotaij

// Handleriotaij is a synthetic struct.
type Handleriotaij struct {
	ID   int
	Name string
}

// Newiotaij returns a new handler.
func Newiotaij() *Handleriotaij {
	return &Handleriotaij{ID: 1, Name: "iotaij"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaij) ProcessRequest(req string) string {
	return req
}
