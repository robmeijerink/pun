<?php

declare(strict_types=1);

/*
 * The SPDX license list Composer 2.11.2 checks a composer.json's `license` against (composer/spdx-licenses, bundled in
 * .oracle/composer/composer.phar). Writes crates/pun-packages/assets/spdx.json: {licenses: {identifier: deprecated},
 * exceptions: [identifier]}, in the bundled files' order.
 *
 *   php scripts/export-spdx.php
 */

$root = dirname(__DIR__);
$phar = 'phar://'.$root.'/.oracle/composer/composer.phar/vendor/composer/spdx-licenses/res/';
$licenses = json_decode(file_get_contents($phar.'spdx-licenses.json'), true, flags: JSON_THROW_ON_ERROR);
$exceptions = json_decode(file_get_contents($phar.'spdx-exceptions.json'), true, flags: JSON_THROW_ON_ERROR);

$out = [
    'licenses' => array_map(static fn (array $license): bool => $license[2], $licenses),
    'exceptions' => array_keys($exceptions),
];
file_put_contents(
    $root.'/crates/pun-packages/assets/spdx.json',
    json_encode($out, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n",
);
printf("%d licenses (%d deprecated), %d exceptions\n", count($licenses), count(array_filter($out['licenses'])), count($exceptions));
