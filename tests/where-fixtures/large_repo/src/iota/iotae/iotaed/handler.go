package iotaed

// Handleriotaed is a synthetic struct.
type Handleriotaed struct {
	ID   int
	Name string
}

// Newiotaed returns a new handler.
func Newiotaed() *Handleriotaed {
	return &Handleriotaed{ID: 1, Name: "iotaed"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaed) ProcessRequest(req string) string {
	return req
}
