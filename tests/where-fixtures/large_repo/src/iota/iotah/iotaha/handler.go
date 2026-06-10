package iotaha

// Handleriotaha is a synthetic struct.
type Handleriotaha struct {
	ID   int
	Name string
}

// Newiotaha returns a new handler.
func Newiotaha() *Handleriotaha {
	return &Handleriotaha{ID: 1, Name: "iotaha"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaha) ProcessRequest(req string) string {
	return req
}
