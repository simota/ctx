package iotabb

// Handleriotabb is a synthetic struct.
type Handleriotabb struct {
	ID   int
	Name string
}

// Newiotabb returns a new handler.
func Newiotabb() *Handleriotabb {
	return &Handleriotabb{ID: 1, Name: "iotabb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabb) ProcessRequest(req string) string {
	return req
}
