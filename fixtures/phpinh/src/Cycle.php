<?php

namespace App\Inh;

// Source-invalid cycle; must not loop.
class Cyc1 extends Cyc2 {
    public function c1m() {}
}
class Cyc2 extends Cyc1 {
    public function c2m() {}
}
class CycUser extends Cyc1 {
    public function f() {
        $this->never_found();   // cycle-safe exhaustion
        $this->c2m();           // reachable through the cycle's Cyc2 arm
    }
}
