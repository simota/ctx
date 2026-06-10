package iotahb

// Handleriotahb is a synthetic struct.
type Handleriotahb struct {
	ID   int
	Name string
}

// Newiotahb returns a new handler.
func Newiotahb() *Handleriotahb {
	return &Handleriotahb{ID: 1, Name: "iotahb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotahb) ProcessRequest(req string) string {
	return req
}
