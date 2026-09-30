<?php
namespace App\Service;

use App\Util;
use App\Service\Helper as Alias;
use function App\Util\slugify;
use function App\Util\slugify as sl;

class Helper {
    public static function run(): int { return 1; }
}

class Repo {
    private string $name = '';

    public static function create(): self { return new self(); }

    public function save(string $key): int {
        $k = slugify($key);            // use function import
        $l = sl($key);                 // use function alias
        $v = Util\version();           // qualified via class-import alias prefix
        $this->persist($k);            // lexical $this direct method
        $this->missing($k);            // no direct method (inherited/magic gap)
        \App\Util\slugify($l);         // fully qualified function
        new Repo();                    // same-namespace construction
        new Alias();                   // aliased class construction
        new \App\Service\Helper();     // FQ construction
        Helper::run();                 // literal static method
        Alias::run();                  // aliased static method
        self::create();                // self:: scoped call
        static::create();              // late static binding -> out of scope
        new static();                  // late static binding -> out of scope
        new parent();                  // unmodeled in V1 -> out of scope
        return (int)$v;
    }

    private function persist(string $k): void {}
}

class Child extends Repo {
    public function go(): void { parent::save('x'); }
}
