<?php

namespace App\Inh;

class TypedCalls {
    public function f(A $a, B $b, D $d) {
        $a->base();             // direct: A::base
        $b->base();             // inherited: A::base via B -> A
        $d->over();             // direct: D::over (override wins over A::over)
        $d->deep();             // inherited depth 3
        $a->hidden();           // private ancestor method -> no candidate
        $b->missing();          // walk exhausted
    }
}

class NewParentCtor {
    public function mk() {
        $x = new \App\Inh\B();
        $x->base();             // local literal-new -> B -> inherited A::base
    }
}
