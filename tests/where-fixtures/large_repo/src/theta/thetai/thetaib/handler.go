package thetaib

// Handlerthetaib is a synthetic struct.
type Handlerthetaib struct {
	ID   int
	Name string
}

// Newthetaib returns a new handler.
func Newthetaib() *Handlerthetaib {
	return &Handlerthetaib{ID: 1, Name: "thetaib"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerthetaib) ProcessRequest(req string) string {
	return req
}
