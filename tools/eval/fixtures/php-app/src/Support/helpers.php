<?php
namespace Acme\App\Support;

function esc(string $s): string { return htmlspecialchars($s); }
function log_line(string $m): void {}
function page_slug(string $t): string { return strtolower($t); }
