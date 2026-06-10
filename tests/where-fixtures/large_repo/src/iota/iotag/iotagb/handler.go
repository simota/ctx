package iotagb

// Handleriotagb is a synthetic struct.
type Handleriotagb struct {
	ID   int
	Name string
}

// Newiotagb returns a new handler.
func Newiotagb() *Handleriotagb {
	return &Handleriotagb{ID: 1, Name: "iotagb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotagb) ProcessRequest(req string) string {
	return req
}
