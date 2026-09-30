<?php
namespace App\Vendor;

use Vendor\Pkg\Tool;

class External {
    public function touch(): void {
        Tool::work();                  // external dep not indexed -> no candidate
        unknown_global();              // not defined anywhere -> no candidate
    }
}
