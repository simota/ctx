// Phase 1 parity fixture: clean code with no secrets.
//
// This file exercises the scanner's "happy zero" path. Every line is
// recognisable code-shaped content; none of the 15 patterns should
// fire, and the high-entropy scan (enabled in parity opts) should also
// stay silent because identifiers are dictionary words.

package example

import "fmt"

// Greet writes a friendly message to stdout.
func Greet(name string) {
	fmt.Println("hello", name)
}

// Add returns the sum of two integers.
func Add(a, b int) int {
	return a + b
}

// Constants used by the cache layer.
const (
	DefaultTimeout = 30
	MaxRetries     = 3
)

func main() {
	Greet("world")
	fmt.Println(Add(2, 3))
}
