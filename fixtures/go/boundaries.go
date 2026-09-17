//go:build linux && amd64

// Fixture: Go boundary probes.
package fixture

type Handler interface {
	Serve()
}

type Base struct{}

type Derived struct {
	Base
	Handler
}

func (b Base) Method() {}

// Tree-sitter's Go grammar decides the shape from syntax alone, and the
// discriminator is the *argument count*, not the number of type arguments:
//
//   Generic[int](value)              -> type_conversion_expression
//   Generic[int, string](value)      -> type_conversion_expression
//   Generic[int](value, other)       -> call_expression / index_expression
//   Generic[int, string](a, b)       -> call_expression / type_arguments
//   int64(value)                     -> call_expression
//
// `Generic[int](value)` is simultaneously a valid generic invocation and a
// valid conversion, so RepoDex records every one of these shapes as a
// call-like occurrence and never resolves which one it is. The form label
// (`type_conversion` vs `plain_name` vs `indirect`) reports the grammar's own
// classification.
func genericShapes(value, other int) {
	converted := Generic[int](value)
	singleTypeArgCall := Generic[int, string](value)
	oneTypeArg := Generic[int](value, other)
	twoTypeArgs := Generic[int, string](value, other)
	plainConverted := int64(value)
	qualified := pkg.Generic[int](value)
	member := obj.Generic[int](value)
	_, _, _, _, _, _, _ = converted, singleTypeArgCall, oneTypeArg, twoTypeArgs,
		plainConverted, qualified, member
}

func methodExpression() {
	f := Base.Method
	_ = f
	var d Derived
	d.Method()
	d.Base.Method()
	count := 1
	{
		count := 2
		_ = count
	}
	_ = count
}
