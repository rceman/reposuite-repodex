package main

import (
	"example.com/micro/internal/engine"
)

func main() {
	r := engine.NewResolver()
	_ = r.ResolveKey("k")
}
