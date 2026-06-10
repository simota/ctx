package iotadh

// Handleriotadh is a synthetic struct.
type Handleriotadh struct {
	ID   int
	Name string
}

// Newiotadh returns a new handler.
func Newiotadh() *Handleriotadh {
	return &Handleriotadh{ID: 1, Name: "iotadh"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadh) ProcessRequest(req string) string {
	return req
}
