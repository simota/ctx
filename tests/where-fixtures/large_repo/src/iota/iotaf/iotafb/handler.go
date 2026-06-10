package iotafb

// Handleriotafb is a synthetic struct.
type Handleriotafb struct {
	ID   int
	Name string
}

// Newiotafb returns a new handler.
func Newiotafb() *Handleriotafb {
	return &Handleriotafb{ID: 1, Name: "iotafb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotafb) ProcessRequest(req string) string {
	return req
}
