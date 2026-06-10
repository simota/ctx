// Package gen exercises Go generics, methods, type specs, and dedup.
package gen

import "fmt"

// Number constrains to numeric types.
type Number interface {
	~int | ~float64
}

// Box is a generic container.
type Box[T any] struct {
	value T
}

// Sum adds two numbers.
func Sum[T Number](a, b T) T {
	return a + b
}

// Get returns the boxed value (pointer-receiver method).
func (b *Box[T]) Get() T {
	return b.value
}

// Set stores a value (value-receiver method).
func (b Box[T]) Set(v T) {
	b.value = v
}

func helper() {
	fmt.Println("helper")
}

// grouped type specs: both extracted as `type`.
type (
	Pair struct {
		A, B int
	}
	Triple struct {
		A, B, C int
	}
)
