package deltabb

// Handlerdeltabb is a synthetic struct.
type Handlerdeltabb struct {
	ID   int
	Name string
}

// Newdeltabb returns a new handler.
func Newdeltabb() *Handlerdeltabb {
	return &Handlerdeltabb{ID: 1, Name: "deltabb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerdeltabb) ProcessRequest(req string) string {
	return req
}
