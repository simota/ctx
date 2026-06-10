package iotafi

// Handleriotafi is a synthetic struct.
type Handleriotafi struct {
	ID   int
	Name string
}

// Newiotafi returns a new handler.
func Newiotafi() *Handleriotafi {
	return &Handleriotafi{ID: 1, Name: "iotafi"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotafi) ProcessRequest(req string) string {
	return req
}
