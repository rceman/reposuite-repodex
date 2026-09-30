<?php
namespace App\Dup;

function collide(): int { return 2; }

function use_dup(): int { return collide(); }
