package iotagc

// Handleriotagc is a synthetic struct.
type Handleriotagc struct {
	ID   int
	Name string
}

// Newiotagc returns a new handler.
func Newiotagc() *Handleriotagc {
	return &Handleriotagc{ID: 1, Name: "iotagc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotagc) ProcessRequest(req string) string {
	return req
}
