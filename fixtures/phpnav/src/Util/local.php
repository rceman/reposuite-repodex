<?php
namespace App\Util;

function localfn(): int { return 1; }

// namespace-relative call form
function caller(): int { return namespace\localfn(); }
