<?php

namespace App\Tests;

use PHPUnit\Framework\Assert;
use PHPUnit\Framework\TestCase;

final class Checker
{
    public function check($value): void
    {
        Assert::assertNotNull($value);
    }

    public static function staticCheck($value): void
    {
        Assert::assertNotNull($value);
    }
}

abstract class BaseCase extends TestCase
{
    protected static function verify($value): void
    {
        static::assertTrue($value);
    }

    protected function verifyThis($value): void
    {
        $this->assertTrue($value);
    }
}

final class CallsTest extends BaseCase
{
    public function testHelperObject(): void
    {
        (new Checker())->check(1);
    }

    public function testHelperVariable(): void
    {
        $checker = new Checker();
        $checker->check(1);
    }

    public function testNamedStatic(): void
    {
        Checker::staticCheck(1);
    }

    public function testParentStatic(): void
    {
        parent::verify(true);
    }

    public function testParentThis(): void
    {
        parent::verifyThis(true);
    }

    public function testStatic(): void
    {
        static::verify(true);
    }

    public function testSelf(): void
    {
        self::verify(true);
    }

    public function testThisInherited(): void
    {
        $this->verifyThis(true);
    }
}
