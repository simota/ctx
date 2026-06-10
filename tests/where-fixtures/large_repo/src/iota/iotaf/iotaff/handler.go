package iotaff

// Handleriotaff is a synthetic struct.
type Handleriotaff struct {
	ID   int
	Name string
}

// Newiotaff returns a new handler.
func Newiotaff() *Handleriotaff {
	return &Handleriotaff{ID: 1, Name: "iotaff"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaff) ProcessRequest(req string) string {
	return req
}
