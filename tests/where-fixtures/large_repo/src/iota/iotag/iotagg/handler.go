package iotagg

// Handleriotagg is a synthetic struct.
type Handleriotagg struct {
	ID   int
	Name string
}

// Newiotagg returns a new handler.
func Newiotagg() *Handleriotagg {
	return &Handleriotagg{ID: 1, Name: "iotagg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotagg) ProcessRequest(req string) string {
	return req
}
