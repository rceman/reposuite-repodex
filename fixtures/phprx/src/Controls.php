<?php
namespace App\Rx;

class Controls {
    public function unknown($service): void {
        $service->handle();
    }

    public function dynamic(Service $s, string $m): void {
        $s->{$m}();
    }

    public function chained(Service $s): void {
        $s->factory()->build();
    }

    public function arrow(): void {
        $f = fn(Service $s) => $s->run();
        $f(new Service());
    }
}
