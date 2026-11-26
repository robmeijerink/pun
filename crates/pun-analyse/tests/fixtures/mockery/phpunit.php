<?php

// The PHPUnit and Mockery-adapter classes upstream's tests extend, without their bodies. PHPUnit itself is not
// installed in the oracle, so both PHPStan and Pun read these.

namespace PHPUnit\Framework {
    abstract class TestCase
    {
        protected function setUp(): void {}

        public static function assertSame(mixed $expected, mixed $actual, string $message = ''): void {}
    }
}

namespace Mockery\Adapter\Phpunit {
    abstract class MockeryTestCase extends \PHPUnit\Framework\TestCase {}
}
