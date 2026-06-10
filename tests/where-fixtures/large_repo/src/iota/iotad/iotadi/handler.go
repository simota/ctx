package iotadi

// Handleriotadi is a synthetic struct.
type Handleriotadi struct {
	ID   int
	Name string
}

// Newiotadi returns a new handler.
func Newiotadi() *Handleriotadi {
	return &Handleriotadi{ID: 1, Name: "iotadi"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadi) ProcessRequest(req string) string {
	return req
}
