<?php
declare(strict_types = 1);

namespace PunCoverage;

use function PunCoverage\Fns\{covListA, covListB};
use PunCoverage\Fns\{function covMixed, Thing};

#[\Attribute]
class AttrOne
{
}

#[AttrOne]
class Base
{
	public const SHARED = 1;
	public static $stat = 1;
	public $inst = 1;

	public function __construct(...$args)
	{
	}

	public static function staticMethod(): void
	{
	}

	public function instanceMethod(): void
	{
	}
}

class Lacking
{
}

class Child extends Base
{
	public function run(): void
	{
		parent::staticMethod();
		static::staticMethod();
		new parent();
		new self();
		new static();
		echo static::SHARED, parent::SHARED;
		echo $this->stat;
		$name = 'stat';
		echo Base::$$name;
		echo Base::${'stat'};
		echo Base::${$name};
		echo Base::${$name . ''};
		echo self::$inst;
	}
}

class WithMissingParent extends MissingParent
{
}

trait UsedTrait
{
	public function used(): void
	{
		covA();
	}
}

trait UnusedTrait
{
	public function unused(): void
	{
		covA();
	}
}

class UsesTrait
{
	use UsedTrait;
}

/**
 * @param class-string<Base> $class
 */
function classString(string $class, $mixed, \PDO $pdo): void
{
	$class->instanceMethod();
	$mixed->instanceMethod();
	$pdo->query('SELECT 1');
	$either = rand(0, 1) ? new Base() : new Lacking();
	echo $either::SHARED;
	$both = rand(0, 1) ? new Base() : new Child();
	$both->instanceMethod();
	new Unknown();
	new WithMissingParent();
	new class (1) {
	};
	array_map([Base::class], [1]);
	array_map([Base::class, 1], [1]);
}

function deadCode(Base $base): void
{
	return;
	$base->instanceMethod();
}

covA();
covB();
covC(1, 2);
covFlags('not an int');
covFlags(1);
covListA();
covListB();
covMixed();
new Thing();
