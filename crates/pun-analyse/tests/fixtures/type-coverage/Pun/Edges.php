<?php

declare(strict_types=1);

namespace Pun\TypeCoverage;

interface Shape
{
    const SIDES = 0;

    public function area($scale);

    public function name();
}

interface Polygon extends Shape
{
    public function area($scale);

    public function corners($round);
}

abstract class Base implements Polygon
{
    const KIND = 'base';

    public $label;

    /**
     * @var callable
     */
    public $callback;

    public function corners($round)
    {
    }

    abstract public function draw($canvas);
}

/**
 * @property string $magic
 * @method void conjured($spell)
 */
class Parent_
{
}

final class Square extends Base
{
    const KIND = 'square', SIZE = 4;

    const SIDES = 4;

    public $label, $width;

    public $magic;

    public $height = 1, $depth;

    public function __construct(public $side, private int $x = 0, ...$rest)
    {
    }

    public function area($scale)
    {
        $double = function ($n) use ($scale) {
            return $n * 2;
        };
        $triple = fn ($n): int => $n * 3;
        $typed = static fn (int $n) => $n;

        return $double($scale) + $triple($scale);
    }

    public function name()
    {
    }

    public function draw($canvas)
    {
    }

    public function __toString()
    {
        return '';
    }

    public static function make($a, int $b, $c = null): static
    {
        return new static(1);
    }

    /**
     * @param callable(int): int $map
     */
    public function mapped($map, $other)
    {
    }

    /**
     * @param  callable $loose
     */
    public function spaced($loose)
    {
    }
}

final class Child extends Parent_
{
    public $magic;

    public function conjured($spell)
    {
    }
}

trait Shared
{
    const TRAIT_CONSTANT = 1;

    public $traitProperty;

    public function shared($value)
    {
    }

    public function __construct()
    {
    }

    abstract public function needed($x);
}

trait Unused
{
    public function unused($value)
    {
    }
}

final class UsesShared
{
    use Shared {
        shared as protected aliased;
    }

    public function needed($x)
    {
    }
}

enum Suit: string
{
    case Hearts = 'h';

    const DEFAULT = self::Hearts;

    public function label($upper)
    {
    }
}

function helper($a, string $b)
{
}

function nothing()
{
}

/**
 * @var resourceful
 */
$anonymous = new class {
    const ANON = 1;

    /** @var resource|null */
    public $handle;

    public function run($x)
    {
    }
};

final class Ignored
{
    // @phpstan-ignore typeCoverage.returnTypeCoverage
    public function ignored()
    {
    }

    public function ignoredLine() // @phpstan-ignore-line
    {
    }
}

final class Attributed
{
    #[\Deprecated]
    public function attributed(
        $first,
        #[\SensitiveParameter] $second,
    ) {
    }

    #[Marker]
    public $attributedProperty;
}

#[\Attribute]
final class Marker
{
}
