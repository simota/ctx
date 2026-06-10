package iotadd

// Handleriotadd is a synthetic struct.
type Handleriotadd struct {
	ID   int
	Name string
}

// Newiotadd returns a new handler.
func Newiotadd() *Handleriotadd {
	return &Handleriotadd{ID: 1, Name: "iotadd"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadd) ProcessRequest(req string) string {
	return req
}
