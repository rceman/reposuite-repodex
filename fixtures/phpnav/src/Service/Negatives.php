<?php
namespace App\Service;

class Negatives {
    public function run(Repo $repo, callable $cb, string $method): void {
        $repo->save('a');              // unknown receiver -> out of scope
        $repo?->save('b');             // nullsafe unknown receiver -> out of scope
        $this->{$method}();            // dynamic member name -> out of scope
        $this->$method();              // dynamic member name -> out of scope
        $cb();                         // variable callable -> out of scope
        $this->dyn();                  // $this but no such direct method
        $this->pages->renderHome();    // property receiver -> out of scope
        Helper::{$method}();           // dynamic scoped member -> out of scope
        $cls = Repo::class;
        new $cls();                    // dynamic class expression -> out of scope
        $f = slugify(...);             // first-class callable: NOT an invocation
        $g = Helper::run(...);         // first-class callable: NOT an invocation
    }
    private function dyn(): void {}
}
