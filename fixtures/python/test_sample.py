"""Fixture: pytest-style and unittest-style test candidates."""

import pytest

from unittest import TestCase


def test_plain():
    pass


@pytest.mark.parametrize("value", [1, 2])
def test_parametrized(value):
    assert value


class TestGroup:
    def test_method(self):
        pass

    def helper_method(self):
        pass


class HelperGroup:
    def test_looking(self):
        pass


class LegacyCase(TestCase):
    def test_legacy(self):
        pass

    def setUp(self):
        pass


def helper_not_a_test():
    pass
