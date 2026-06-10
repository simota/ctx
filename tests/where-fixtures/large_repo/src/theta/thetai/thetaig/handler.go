package thetaig

// Handlerthetaig is a synthetic struct.
type Handlerthetaig struct {
	ID   int
	Name string
}

// Newthetaig returns a new handler.
func Newthetaig() *Handlerthetaig {
	return &Handlerthetaig{ID: 1, Name: "thetaig"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerthetaig) ProcessRequest(req string) string {
	return req
}
