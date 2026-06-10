package iotajh

// Handleriotajh is a synthetic struct.
type Handleriotajh struct {
	ID   int
	Name string
}

// Newiotajh returns a new handler.
func Newiotajh() *Handleriotajh {
	return &Handleriotajh{ID: 1, Name: "iotajh"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotajh) ProcessRequest(req string) string {
	return req
}
