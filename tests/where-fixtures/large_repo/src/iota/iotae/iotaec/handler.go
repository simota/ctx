package iotaec

// Handleriotaec is a synthetic struct.
type Handleriotaec struct {
	ID   int
	Name string
}

// Newiotaec returns a new handler.
func Newiotaec() *Handleriotaec {
	return &Handleriotaec{ID: 1, Name: "iotaec"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaec) ProcessRequest(req string) string {
	return req
}
