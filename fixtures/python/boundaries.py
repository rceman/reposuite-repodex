"""Fixture: Python boundary probes."""

import importlib
from unittest import TestCase as AliasedTestCase

UPPERCASE_NAME = 1


def boundaries():
    global UPPERCASE_NAME
    value = 1

    def inner():
        nonlocal value
        return value

    match value:
        case 1:
            pass
        case _:
            pass

    module = importlib.import_module("os")
    getattr(module, "path")()
    return inner


class AliasedCase(AliasedTestCase):
    def test_aliased(self):
        pass


class MonkeyTarget:
    def method(self):
        pass


MonkeyTarget.method = lambda self: None
original = MonkeyTarget.method

# `global`/`nonlocal` statements and `match` bindings are not declarations, and
# an uppercase name is a variable, not a proven constant.
