<?php
namespace Acme\App\Service;

use Acme\App\Repo\Pages;
use function Acme\App\Support\page_slug;
use function Acme\App\Support\log_line;

class PageService {
    public function find(string $title): ?string {
        $slug = page_slug($title);
        log_line('find ' . $slug);
        return Pages::lookup($slug);
    }
    public function renderHome(): string {
        $body = $this->find('home') ?? 'empty';
        return $this->frame($body);
    }
    private function frame(string $body): string {
        return "<main>$body</main>";
    }
}
