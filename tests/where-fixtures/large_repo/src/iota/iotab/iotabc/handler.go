package iotabc

// Handleriotabc is a synthetic struct.
type Handleriotabc struct {
	ID   int
	Name string
}

// Newiotabc returns a new handler.
func Newiotabc() *Handleriotabc {
	return &Handleriotabc{ID: 1, Name: "iotabc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabc) ProcessRequest(req string) string {
	return req
}
