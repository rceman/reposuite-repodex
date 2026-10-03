<?php

namespace App\Inh;

interface ServiceContract {
    public function contract();
}

class Impl implements ServiceContract {
    public function contract() {}
}

trait TLogger {
    public function logline() {}
}

class WithTrait extends Impl {
    use TLogger;
    public function g() {
        $this->contract();      // inherited via Impl? no - Impl has it direct
        $this->logline();       // trait method -> unsupported in V1
    }
}

abstract class AbsBase {
    abstract public function must_impl();
    public function concrete() {}
}

class AbsChild extends AbsBase {
    public function must_impl() {}
    public function h() {
        $this->concrete();      // concrete parent method
    }
}

class ParentHint {
    public function mark() {}
}
class ParentHintChild extends ParentHint {
    public function f(parent $p) {
        $p->mark();             // `parent` type hint -> declared parent
    }
}
