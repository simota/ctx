package iotabe

// Handleriotabe is a synthetic struct.
type Handleriotabe struct {
	ID   int
	Name string
}

// Newiotabe returns a new handler.
func Newiotabe() *Handleriotabe {
	return &Handleriotabe{ID: 1, Name: "iotabe"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabe) ProcessRequest(req string) string {
	return req
}
