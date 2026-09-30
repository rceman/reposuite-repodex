<?php

function global_helper(): string { return 'g'; }

// global-scope caller: unqualified global call
function boot(): string { return global_helper(); }
