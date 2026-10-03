<?php

namespace App\Inh;

class DupChild extends SharedBase {
    public function f() {
        $this->shared_one();    // ambiguous parent: both decls walked
        $this->shared_two();    // both parent arms contribute
    }
}

class ExtChild extends Vendor\Missing {
    public function f() {
        $this->anything();      // external parent -> no candidates
    }
}
