"""Fixture: shared adversarial cases for Python.

UTF-8 multibyte text before a declaration: "αβγ δεζ" and "日本語テキスト".
"""

# Duplicate names in different contexts.


def name():
    return 1


class Duplicate:
    def name(self):
        return 2


class Other:
    def name(self):
        return 3


# Deep nesting.


def outer():
    def level_two():
        def level_three():
            def level_four():
                return 4

            return level_four

        return level_three

    return level_two


# Syntactically valid but semantically invalid: `Missing` is never defined.


def semantically_invalid():
    return Missing(AlsoMissing)


# Generated-looking source.


def generated_0001():
    pass


def generated_0002():
    pass


def generated_0003():
    pass


def generated_0004():
    pass
