package iotaga

// Handleriotaga is a synthetic struct.
type Handleriotaga struct {
	ID   int
	Name string
}

// Newiotaga returns a new handler.
func Newiotaga() *Handleriotaga {
	return &Handleriotaga{ID: 1, Name: "iotaga"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaga) ProcessRequest(req string) string {
	return req
}
