<?php
namespace Acme\App\Tests;

use Acme\App\Http\HomeController;

class HomeControllerTest {
    public function testIndex(): void {
        $c = new HomeController();
        assert(is_string($c->index()));
    }
}
