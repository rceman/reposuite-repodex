<?php
namespace Acme\App\Http;

use Acme\App\Service\PageService;
use function Acme\App\Support\esc;

class HomeController {
    private PageService $pages;

    public function __construct() {
        $this->pages = new PageService();
    }

    public function index(): string {
        $html = $this->pages->renderHome();
        return esc($html);
    }

    public function about(): string {
        $svc = new PageService();
        return $svc->renderHome();
    }
}
