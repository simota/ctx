package iotabd

// Handleriotabd is a synthetic struct.
type Handleriotabd struct {
	ID   int
	Name string
}

// Newiotabd returns a new handler.
func Newiotabd() *Handleriotabd {
	return &Handleriotabd{ID: 1, Name: "iotabd"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabd) ProcessRequest(req string) string {
	return req
}
