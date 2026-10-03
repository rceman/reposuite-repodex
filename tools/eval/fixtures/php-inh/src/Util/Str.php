<?php

namespace App\Util;

function slugify(string $s): string {
    return strtolower($s);
}
