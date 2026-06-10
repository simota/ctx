package db

type Conn struct{}

func Connect(dsn string) (*Conn, error) { return &Conn{}, nil }
func (c *Conn) Close() error { return nil }
