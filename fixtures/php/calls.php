<?php

namespace App\Calls;

class Widget
{
    public static function create(): self
    {
        return new self();
    }

    public function render(): string
    {
        return '';
    }
}

function calls(?Widget $widget): void
{
    plainCall(1, 2);
    $widget?->render();
    $widget->render();
    Widget::create();
    \App\Calls\Widget::create();
    $callable = 'strlen';
    $callable('abc');
    $name = 'render';
    $widget->{$name}();
    $firstClass = strlen(...);
    $closure = function (int $x) use ($firstClass): int {
        return $x;
    };
    $arrow = fn(int $y): int => $y * 2;
    $created = new Widget();
    $defaulted = helperWithDefault();
    $nested = strlen(...)($callable);
    // An anonymous class has no name. The callee is the `class` keyword alone,
    // and its range must cover exactly those five bytes even though the
    // `anonymous_class` node spans the whole body.
    $anonymous = new class() implements \Countable {
        public function count(): int
        {
            return 0;
        }
    };
    // An attribute list precedes `class`, so the `anonymous_class` node starts
    // at `#[`, not at the keyword. The callee range must still be the keyword.
    $attributed = new #[Marker('x')] class() implements \Countable {
        public function count(): int
        {
            return 1;
        }
    };
}
