<?php

namespace App\Tests;

use PHPUnit\Framework\Attributes\Test;
use PHPUnit\Framework\TestCase;

final class ServiceTest extends TestCase
{
    public function testSomething(): void
    {
    }

    #[Test]
    public function annotatedMethod(): void
    {
    }

    public function helperMethod(): void
    {
    }
}

final class HelperClass
{
    public function testLooking(): void
    {
    }
}
