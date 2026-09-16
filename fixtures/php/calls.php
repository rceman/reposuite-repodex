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
}
