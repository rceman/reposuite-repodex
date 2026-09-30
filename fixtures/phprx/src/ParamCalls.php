<?php
namespace App\Rx;

use App\Rx\Service as Svc;
use Vendor\Pkg\Tool;

class ParamCaller {
    public function direct(Service $service): void {
        $service->handle();
    }

    public function aliased(Svc $s): void {
        $s->run();
    }

    public function qualified(\App\Rx\Repo $r): void {
        $r->save();
    }

    public function nullable(?Cache $c): void {
        $c?->get();
    }

    public function unioned(Service|Cache $x): void {
        $x->run();
        $x->get();
    }

    public function intersected(Repo&Cache $x): void {
        $x->save();
    }

    public function builtin(int $n): void {
        $n->abs();
    }

    public function selfTyped(self $s): void {
        $s->helper();
    }

    public function parentTyped(parent $p): void {
        $p->anything();
    }

    public function overwritten(Service $x): void {
        $x = dynMake();
        $x->run();
    }

    public function reassigned(Service $x): void {
        $x = new Alt();
        $x->go();
    }

    public function sequential(): void {
        $x = new Alt();
        $x = new Local();
        $x->go();
    }

    public function branched(bool $flag): void {
        if ($flag) {
            $x = new Alt();
        } else {
            $x = new Local();
        }
        $x->go();
    }

    public function lateOpaque(): void {
        $x = new Local();
        $x = weird();
        $x->go();
    }

    public function external(Tool $t): void {
        $t->work();
    }

    public function duplicated(Dup\Thing $t): void {
        $t->m();
    }

    private function helper(): void {}
}
