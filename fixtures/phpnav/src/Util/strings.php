<?php
namespace App\Util;

function slugify(string $s): string { return strtolower($s); }
function version(): string { return '1.0'; }

// same-namespace unqualified call (tier: current_namespace)
function describe(): string { return version(); }

// global-fallback tier: no App\Util\global_helper exists -> \global_helper
function fallback(): string { return global_helper(); }
