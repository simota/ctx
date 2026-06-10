package iotahd

// Handleriotahd is a synthetic struct.
type Handleriotahd struct {
	ID   int
	Name string
}

// Newiotahd returns a new handler.
func Newiotahd() *Handleriotahd {
	return &Handleriotahd{ID: 1, Name: "iotahd"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotahd) ProcessRequest(req string) string {
	return req
}
