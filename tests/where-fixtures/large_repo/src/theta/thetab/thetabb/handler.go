package thetabb

// Handlerthetabb is a synthetic struct.
type Handlerthetabb struct {
	ID   int
	Name string
}

// Newthetabb returns a new handler.
func Newthetabb() *Handlerthetabb {
	return &Handlerthetabb{ID: 1, Name: "thetabb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerthetabb) ProcessRequest(req string) string {
	return req
}
