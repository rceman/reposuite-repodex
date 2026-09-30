<?php
namespace Acme\TextLib\Text;

function slugify(string $s): string { return strtolower(trim($s)); }
function words(string $s): array { return explode(' ', $s); }
function truncate(string $s, int $n): string { return strlen($s) > $n ? substr($s, 0, $n) : $s; }
function version(): string { return '2.1'; }
