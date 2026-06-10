package iotahi

// Handleriotahi is a synthetic struct.
type Handleriotahi struct {
	ID   int
	Name string
}

// Newiotahi returns a new handler.
func Newiotahi() *Handleriotahi {
	return &Handleriotahi{ID: 1, Name: "iotahi"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotahi) ProcessRequest(req string) string {
	return req
}
