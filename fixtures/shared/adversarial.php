<?php

// Fixture: shared adversarial cases for PHP.
//
// UTF-8 multibyte text before a declaration: "αβγ δεζ" and "日本語テキスト".

namespace App\Adversarial;

// Duplicate names in different contexts.

class Duplicate
{
    public function name(): string
    {
        return 'method';
    }
}

class Other
{
    public function name(): string
    {
        return 'other';
    }
}

trait Naming
{
    public function name(): string
    {
        return 'trait';
    }
}

function name(): string
{
    return 'function';
}

// Deep nesting: closures, not declarations.

function deepNesting(): void
{
    $level1 = function () {
        $level2 = function () {
            $level3 = function () {
                $level4 = function () {
                };
                $level4();
            };
            $level3();
        };
        $level2();
    };
    $level1();
}

// Syntactically valid but semantically invalid: `Missing` is never declared.

function semanticallyInvalid(): Missing
{
    return new AlsoMissing();
}

// Generated-looking source.

function generated0001(): void
{
}

function generated0002(): void
{
}

function generated0003(): void
{
}

function generated0004(): void
{
}
