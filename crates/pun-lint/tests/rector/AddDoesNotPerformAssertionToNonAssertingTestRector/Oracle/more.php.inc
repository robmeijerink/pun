<?php

namespace App\Tests;

use PHPUnit\Framework\Assert;
use PHPUnit\Framework\Attributes\DoesNotPerformAssertions as NoAsserts;
use PHPUnit\Framework\MockObject\Stub;
use PHPUnit\Framework\TestCase;

final class Checker
{
    public function check($value): void
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
}

final class MoreTest extends BaseCase
{
    private Stub $stub;

    public function testHelperObject(): void
    {
        (new Checker())->check(1);
    }

    public function testParentHelper(): void
    {
        parent::verify(true);
    }

    /**
     * Runs only for coverage.
     *
     * @test
     */
    public function runs(): void
    {
        $this->addToAssertionCount(1);
        $x = 1;
        $this->addToAssertionCount(2);
    }

    public function testOnlyCount(): void
    {
        $this->addToAssertionCount(1);
    }

    #[NoAsserts]
    public function testAliased(): void
    {
    }

    public function testStub(): void
    {
        $this->stub->method('a');
    }

    public function testClosure(): void
    {
        $f = function () {
            $this->addToAssertionCount(1);
        };
        $f();
    }

    public function provideData(): array
    {
        return [];
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\Assert;
use PHPUnit\Framework\Attributes\DoesNotPerformAssertions as NoAsserts;
use PHPUnit\Framework\MockObject\Stub;
use PHPUnit\Framework\TestCase;

final class Checker
{
    public function check($value): void
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
}

final class MoreTest extends BaseCase
{
    private Stub $stub;

    public function testHelperObject(): void
    {
        (new Checker())->check(1);
    }

    public function testParentHelper(): void
    {
        parent::verify(true);
    }

    /**
     * Runs only for coverage.
     *
     * @test
     */
    #[\PHPUnit\Framework\Attributes\DoesNotPerformAssertions]
    public function runs(): void
    {
        $x = 1;
    }

    #[\PHPUnit\Framework\Attributes\DoesNotPerformAssertions]
    public function testOnlyCount(): void
    {
    }

    #[NoAsserts]
    public function testAliased(): void
    {
    }

    public function testStub(): void
    {
        $this->stub->method('a');
    }

    #[\PHPUnit\Framework\Attributes\DoesNotPerformAssertions]
    public function testClosure(): void
    {
        $f = function () {
        };
        $f();
    }

    public function provideData(): array
    {
        return [];
    }
}
