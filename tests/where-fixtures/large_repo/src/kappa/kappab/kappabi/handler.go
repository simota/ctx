package kappabi

// Handlerkappabi is a synthetic struct.
type Handlerkappabi struct {
	ID   int
	Name string
}

// Newkappabi returns a new handler.
func Newkappabi() *Handlerkappabi {
	return &Handlerkappabi{ID: 1, Name: "kappabi"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerkappabi) ProcessRequest(req string) string {
	return req
}
