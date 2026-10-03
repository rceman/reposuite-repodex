<?php

namespace App\Core;

use App\Page\PageRenderer;

abstract class Controller {
    protected PageRenderer $renderer;

    public function __construct() {
        $this->renderer = new PageRenderer();
    }

    public function render(string $view, array $ctx): string {
        return $this->renderer->draw($view, $ctx);
    }

    public function authenticate(): bool {
        return true;
    }
}
