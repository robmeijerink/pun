<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class RegExpMoreTest extends TestCase
{
    public function test(string $string)
    {
        self::assertSame(0, preg_match('/a/', $string));
        $this->assertNotEquals(false, \preg_match('/a/', $string, $matches), 'message');
        $this->assertSame(2, preg_match('/b/', $string));
        $this->assertEquals(TRUE, preg_match('/c/', $string));
        $this->assertSame(1, preg_match('/d/', $string, $found));
        $this->assertTrue(true);
        $this->assertSame(1, strlen($string));
        $this->assertSame(1);
    }

    public function testMatchesUsedLater(string $string)
    {
        if (true) {
            $this->assertSame(1, preg_match('/e/', $string, $parts));
            echo $parts[0];
        }
    }
}

final class RegExpNotTest
{
    public function test(string $string)
    {
        $this->assertSame(1, preg_match('/a/', $string));
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class RegExpMoreTest extends TestCase
{
    public function test(string $string)
    {
        self::assertDoesNotMatchRegularExpression('/a/', $string);
        $this->assertMatchesRegularExpression('/a/', $string, 'message');
        $this->assertSame(2, preg_match('/b/', $string));
        $this->assertMatchesRegularExpression('/c/', $string);
        $this->assertMatchesRegularExpression('/d/', $string);
        $this->assertTrue(true);
        $this->assertSame(1, strlen($string));
        $this->assertSame(1);
    }

    public function testMatchesUsedLater(string $string)
    {
        if (true) {
            $this->assertSame(1, preg_match('/e/', $string, $parts));
            echo $parts[0];
        }
    }
}

final class RegExpNotTest
{
    public function test(string $string)
    {
        $this->assertSame(1, preg_match('/a/', $string));
    }
}
