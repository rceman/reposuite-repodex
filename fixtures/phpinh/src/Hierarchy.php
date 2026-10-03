<?php

namespace App\Inh;

// Three-level chain: A -> B -> C -> D with overrides at different levels.

class A {
    public function base() {}
    public function over() {}
    protected function prot() {}
    private function hidden() {}
    public function deep() {}
}

class B extends A {
    public function over() {}
    public function only_b() {}
}

class C extends B {
}

class D extends C {
    public function over() {}
    public function use_it() {
        $this->base();          // level 3 up: A::base
        $this->over();          // level 0: D::over (override wins)
        $this->only_b();        // level 2: B::only_b
        $this->prot();          // level 3: A::prot (protected reachable)
        $this->hidden();        // private on A -> NOT callable -> no candidate
        $this->deep();          // level 3: A::deep
        $this->missing();       // absent everywhere
        self::prot();           // self-scoped inherited
        parent::over();         // parent:: -> C has none -> B::over
        new parent();           // C candidate
        static::later();        // OOS late static
        new static();           // OOS late static
    }
}

final class Beeper extends A {
}

class AliasChild extends \App\Inh\B {
    public function f() {
        $this->only_b();        // aliased/FQN parent -> B::only_b
    }
}
