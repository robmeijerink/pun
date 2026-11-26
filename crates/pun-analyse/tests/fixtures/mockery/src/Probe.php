<?php declare(strict_types = 1);

namespace PHPStan\Mockery;

use Mockery;
use Mockery\LegacyMockInterface;
use Mockery\MockInterface;

use function PHPStan\dumpType;

final class Probe
{
    /** @var MockInterface|Foo */
    public $property;

    /** @var Foo|MockInterface */
    public $reversed;

    /** @var MockInterface|null */
    public $nullable;

    /** @var MockInterface|Foo|null */
    public $three;

    /** @var LegacyMockInterface|Foo */
    public $legacy;

    /** @var Foo|Bar */
    public $plain;

    public function mocks(string $name): void
    {
        dumpType(Mockery::mock());
        dumpType(Mockery::mock(Foo::class));
        dumpType(Mockery::mock('alias:' . Foo2::class));
        dumpType(Mockery::mock('overload:' . Foo3::class));
        dumpType(Mockery::mock(Foo::class . '[doFoo]'));
        dumpType(Mockery::mock(Foo::class, Baz::class . ', ' . Buzz::class));
        dumpType(Mockery::mock(Foo::class, [true]));
        dumpType(Mockery::mock(Baz::class, Buzz::class));
        dumpType(Mockery::mock($name));
        dumpType(Mockery::mock(rand(0, 1) === 1 ? Foo::class : Baz::class));
        dumpType(Mockery::spy());
        dumpType(Mockery::spy(Foo::class));
        dumpType(Mockery::namedMock('FooBar', Foo::class));
        dumpType(Mockery::namedMock(Foo::class));
        dumpType(Mockery::namedMock('FooBar', Foo::class, Buzz::class));
        dumpType(Mockery::instanceMock(Foo::class));
        dumpType(\mock(Foo::class));
    }

    /**
     * @param MockInterface&Foo $mock
     */
    public function stubs(MockInterface $plain, $mock): void
    {
        dumpType($mock->allows());
        dumpType($mock->expects());
        dumpType($mock->allows()->doFoo());
        dumpType($mock->expects()->doFoo(1, 2));
        dumpType($mock->allows('doFoo'));
        dumpType($mock->expects('doFoo'));
        dumpType($mock->allows(['doFoo' => 'bar']));
        dumpType($plain->allows());
        dumpType($plain->expects());
        dumpType($mock->makePartial());
        dumpType($mock->shouldAllowMockingProtectedMethods());
        dumpType($mock->shouldReceive('doFoo'));
        dumpType($mock->shouldReceive());
        dumpType($mock->shouldNotReceive('doFoo'));
        dumpType($mock->shouldHaveReceived('doFoo'));
        dumpType($mock->shouldNotHaveReceived(null));
        dumpType(Mockery::mock(Foo::class)->allows()->doFoo());
    }

    /**
     * @param MockInterface|Foo $param
     * @return MockInterface|Baz
     */
    public function phpDoc($param)
    {
        dumpType($this->property);
        dumpType($this->reversed);
        dumpType($this->nullable);
        dumpType($this->three);
        dumpType($this->legacy);
        dumpType($this->plain);
        dumpType($param);
        dumpType($this->phpDoc($param));

        /** @var MockInterface|Buzz $inline */
        $inline = Mockery::mock(Buzz::class);
        dumpType($inline);

        return Mockery::mock(Baz::class);
    }
}
