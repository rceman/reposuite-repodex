<?php
namespace Acme\App\Http;

class Negatives {
    public function dispatch($service, callable $cb, string $m): void {
        $service->handle();            // unknown receiver
        $this->$m();                   // dynamic member name
        $cb();                         // variable callable
        static::boot();                // late static binding
        $f = strlen(...);              // first-class callable
    }
    public static function boot(): void {}
}
