<?php
namespace Acme\TextLib\Store;

use Acme\TextLib\Text\Shout;

class Cache {
    public static function make(): self {
        return new self();
    }
    public function label(string $s): string {
        return Shout::of($s);
    }
}
