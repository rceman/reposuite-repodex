<?php
namespace App\Page;

class PageRenderer {
    public function render(string $v): string { return $v; }
    public function renderHome(): string { return 'home'; }
}
