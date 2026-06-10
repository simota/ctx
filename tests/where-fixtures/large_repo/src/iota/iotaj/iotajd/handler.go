package iotajd

// Handleriotajd is a synthetic struct.
type Handleriotajd struct {
	ID   int
	Name string
}

// Newiotajd returns a new handler.
func Newiotajd() *Handleriotajd {
	return &Handleriotajd{ID: 1, Name: "iotajd"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotajd) ProcessRequest(req string) string {
	return req
}
