package main

import (
	"example.com/fixture/internal/foo"
	"example.com/fixture/internal/two"
	"example.com/fixture/internal/missing"
	"example.com/fixture/internal"
	"example.com/fixture"
	"example.com/fixturex"
	"fmt"
	_ "example.com/fixture/internal/bar"
)

func main() {
	foo.F()
	_ = fmt.Sprint
}
