"""Fixture: Python declarations and lexical containment."""

import os
import os, sys
import numpy as np
from . import sibling
from ..pkg import thing
from pkg.sub import first, second as renamed
from pkg import *
from typing import Final

MODULE_X = 1
MODULE_Y: int = 2
MODULE_Z: Final = 3
MODULE_A, MODULE_B = 4, 5


@decorator
@decorator_with_args(1)
class Base:
    class_attr = 1

    def __init__(self, value=default_value()):
        self.value = value

    async def fetch(self):
        return await get_value()

    def helper(self):
        pass


class Child(Base, mixins.Other):
    def method(self):
        pass


async def top_async(a, b=1):
    pass


def top(a, b=default_for_top(), *args, c: int = 2, **kwargs):
    def nested():
        pass

    class Nested:
        pass

    lam = lambda q: q + 1
    return lam
