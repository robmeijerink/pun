<?php

declare(strict_types=1);

/*
 * What PHP itself answers for the two checks Composer's ValidatingArrayLoader makes on URLs and emails, for the ports in
 * crates/pun-packages/src/php.rs. Writes crates/pun-packages/tests/fixtures/php-filters.json:
 * {email: {input: valid}, url: {input: {scheme, host} | false}} (parse_url's scheme and host, or false when it fails).
 *
 *   php scripts/export-php-filters.php
 */

$emails = [
    'a@example.com', 'first.last@example.com', 'a+tag@example.co.uk', 'user@sub.example.com', 'x@localhost',
    'not-an-email', '@example.com', 'a@', 'a@@example.com', 'a b@example.com', '"a b"@example.com', '"a\\"b"@example.com',
    'a.@example.com', '.a@example.com', 'a..b@example.com', 'a@example', 'a@-example.com', 'a@example-.com',
    'a@exa_mple.com', 'a@[127.0.0.1]', 'a@[IPv6:::1]', 'a@[IPv6:2001:db8::1]', 'a@[256.0.0.1]', 'a@example.c',
    'a@1example.com', 'a@example.123', 'a@xn--bcher-kva.example', 'a@EXAMPLE.COM', "a@example.com\n", ' a@example.com',
    'a@example.com ', 'ünicode@example.com', 'a@ünicode.com', str_repeat('a', 64).'@example.com',
    str_repeat('a', 65).'@example.com', 'a@'.str_repeat('b', 63).'.com', 'a@'.str_repeat('b', 64).'.com',
    'a@'.implode('.', array_fill(0, 60, 'abcd')).'.com', "a!#$%&'*+/=?^_`{|}~-@example.com", 'a(b)@example.com',
    'a@example.com.', 'a@.example.com', '', 'a@b.c.d.e.f.gh',
];

$urls = [
    'https://example.com', 'http://example.com/path?q=1#f', 'HTTPS://EXAMPLE.COM', 'ftp://example.com', 'example.com',
    '//example.com', 'http://', 'http:///path', 'http:/example.com', 'http:example.com', 'https://user:pass@example.com:8080/x',
    'https://example.com:99999', 'https://example.com:0', 'https://example.com:abc', 'https://[::1]:8080/', 'https://[::1',
    'irc://irc.libera.chat/composer', 'ircs://irc.libera.chat:6697/x', 'mailto:a@example.com', 'https://ex ample.com',
    'https://example.com:', 'https://@example.com', 'h-t.tp+s://example.com', '1http://example.com', 'http//example.com',
    'localhost:8080', 'https://exa mple.com/path', "https://example.com\n", 'https://example.com/a b', 'https://ünicode.com',
    'https://example.com?x', 'https://example.com#x', 'https:// example.com', 'https://example.com:80:80', 'javascript:alert(1)',
    'https:', ':https://example.com', 'https://a@b@c.com', 'https://example.com/../x', 'http://example.com:65535',
    'http://example.com:65536', 'https://exam%20ple.com',
];

$out = ['email' => [], 'url' => []];
foreach ($emails as $email) {
    $out['email'][$email] = false !== filter_var($email, FILTER_VALIDATE_EMAIL);
}
foreach ($urls as $url) {
    $bits = parse_url($url);
    $out['url'][$url] = false === $bits ? false : ['scheme' => $bits['scheme'] ?? null, 'host' => $bits['host'] ?? null];
}
file_put_contents(
    dirname(__DIR__).'/crates/pun-packages/tests/fixtures/php-filters.json',
    json_encode($out, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR)."\n",
);
printf("PHP %s: %d emails, %d urls\n", PHP_VERSION, count($emails), count($urls));
