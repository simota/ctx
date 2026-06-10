package thetaca

// Handlerthetaca is a synthetic struct.
type Handlerthetaca struct {
	ID   int
	Name string
}

// Newthetaca returns a new handler.
func Newthetaca() *Handlerthetaca {
	return &Handlerthetaca{ID: 1, Name: "thetaca"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerthetaca) ProcessRequest(req string) string {
	return req
}
