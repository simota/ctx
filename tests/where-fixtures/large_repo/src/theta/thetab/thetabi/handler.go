package thetabi

// Handlerthetabi is a synthetic struct.
type Handlerthetabi struct {
	ID   int
	Name string
}

// Newthetabi returns a new handler.
func Newthetabi() *Handlerthetabi {
	return &Handlerthetabi{ID: 1, Name: "thetabi"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerthetabi) ProcessRequest(req string) string {
	return req
}
