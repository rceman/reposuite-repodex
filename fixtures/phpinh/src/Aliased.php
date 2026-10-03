<?php

namespace App\Inh;

use App\Inh\B as BBase;

class ViaAlias extends BBase {
    public function go() {
        $this->only_b();        // alias-resolved parent -> B::only_b
    }
}

class ViaRel extends Inh\A {    // namespace-relative-ish written name
}
