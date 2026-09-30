<?php
namespace App\Rx;

class Repo {
    public function save(): void {}
    public function query(): array { return []; }
}

class Cache {
    public function get(): ?string { return null; }
}

class Service {
    public function run(): void {}
    public function handle(): int { return 0; }
}

class Pages {
    public function render(): string { return ''; }
}

class Local {
    public function go(): void {}
}

class Alt {
    public function go(): void {}
}
