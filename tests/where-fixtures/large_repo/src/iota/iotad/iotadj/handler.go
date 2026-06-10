package iotadj

// Handleriotadj is a synthetic struct.
type Handleriotadj struct {
	ID   int
	Name string
}

// Newiotadj returns a new handler.
func Newiotadj() *Handleriotadj {
	return &Handleriotadj{ID: 1, Name: "iotadj"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadj) ProcessRequest(req string) string {
	return req
}
