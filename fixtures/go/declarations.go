// Fixture: Go declarations and lexical containment.
package fixture

import (
	"fmt"
	alias "example.com/alias"
	_ "example.com/blank"
	. "example.com/dot"
)

import "example.com/single"

type UserID string

type Alias = string

// A generic type parameter is syntax, not a declaration. `Box` is declared;
// `T` and `any` are not.
type Box[T any] struct {
	Value T
}

type Pair[K comparable, V any] struct {
	Key   K
	Value V
	Embedded
	*Pointer
}

type Reader interface {
	Read(p []byte) (int, error)
	Closer
}

const (
	StatusA = iota
	StatusB
	StatusC, StatusD = 1, 2
)

const Single = 1

var Global int

var X, Y = 1, 2

func Free[T any](value T) T {
	return value
}

func (u UserID) Value() string {
	return string(u)
}

func (u *UserID) Set() {}

func outer() {
	inner := func() {}
	inner()

	func() {
		nested := func() {}
		nested()
	}()
}
