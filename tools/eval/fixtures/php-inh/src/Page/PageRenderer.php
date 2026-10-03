<?php

namespace App\Page;

class PageRenderer {
    public function draw(string $view, array $ctx): string {
        return "<{$view}>";
    }
}

class HtmlView {
    public function render(string $view, array $ctx): string {
        return '';
    }
}
