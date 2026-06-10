package deltach

// Handlerdeltach is a synthetic struct.
type Handlerdeltach struct {
	ID   int
	Name string
}

// Newdeltach returns a new handler.
func Newdeltach() *Handlerdeltach {
	return &Handlerdeltach{ID: 1, Name: "deltach"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerdeltach) ProcessRequest(req string) string {
	return req
}
