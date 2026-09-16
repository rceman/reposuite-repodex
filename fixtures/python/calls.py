"""Fixture: Python call-shaped syntax."""


def calls():
    plain(1, 2)
    obj.attribute_call(3)
    obj.chained().call(4)
    indirect = get_callable()
    indirect(5)
    getattr(obj, "method")(6)
    rendered = f"{render(7)}"
    values = [transform(v) for v in items]
    return rendered, values, len(values)
