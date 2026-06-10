package iotaib

// Handleriotaib is a synthetic struct.
type Handleriotaib struct {
	ID   int
	Name string
}

// Newiotaib returns a new handler.
func Newiotaib() *Handleriotaib {
	return &Handleriotaib{ID: 1, Name: "iotaib"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaib) ProcessRequest(req string) string {
	return req
}
