<?php
namespace App\Rx;

class PropCaller {
    private Service $svc;
    private ?Repo $maybe;
    private $plain;

    public function __construct(private Cache $cache) {
        $this->pages = new Pages();
        $this->dup = new Alt();
    }

    public function reassign(): void {
        $this->dup = new Local();
        $this->plain = new Pages();
    }

    public function home(): string {
        return $this->pages->render();
    }

    public function read(): void {
        $this->svc->handle();
        $this->cache->get();
        $this->maybe?->save();
        $this->dup->go();
        $this->plain->render();
    }

    public function untouched(): void {
        $this->missing->run();
        $this->svc->absent();
    }
}
