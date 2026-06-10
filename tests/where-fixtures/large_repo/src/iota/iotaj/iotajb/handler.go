package iotajb

// Handleriotajb is a synthetic struct.
type Handleriotajb struct {
	ID   int
	Name string
}

// Newiotajb returns a new handler.
func Newiotajb() *Handleriotajb {
	return &Handleriotajb{ID: 1, Name: "iotajb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotajb) ProcessRequest(req string) string {
	return req
}
