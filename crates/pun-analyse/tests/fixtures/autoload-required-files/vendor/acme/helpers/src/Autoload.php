<?php

require_once __DIR__.'/functions.php';

// require_once __DIR__.'/commented.php';

(static function () {
    $files = [
        'Authentication.php',
        'Database.php',
    ];

    foreach ($files as $file) {
        require_once __DIR__."/{$file}";
    }
})();
