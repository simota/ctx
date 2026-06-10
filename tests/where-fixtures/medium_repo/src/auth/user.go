package auth

type User struct {
	ID    string
	Email string
}

func GetUserByID(id string) *User { return &User{ID: id} }
func GetUserByEmail(email string) *User { return &User{Email: email} }
