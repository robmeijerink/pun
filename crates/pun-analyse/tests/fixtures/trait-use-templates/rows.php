<?php

declare(strict_types=1);

namespace TraitUseTemplates;

final class Apple
{
    public function __construct(public string $name) {}
}

final class Pear {}

/**
 * @template TRow of object
 */
trait HoldsRows
{
    /** @var list<TRow> */
    private array $rows = [];

    /** @param  list<TRow>  $rows */
    abstract protected function describe(array $rows): string;

    /** @param  list<TRow>  $rows */
    public function hold(array $rows): string
    {
        $this->rows = $rows;

        return $this->describe($rows);
    }

    /**
     * @template TPicked of TRow
     *
     * @param  TPicked  $row
     * @return TPicked
     */
    public function keep(object $row): object
    {
        return $row;
    }

    /** @return TRow|null */
    public function firstRow(): ?object
    {
        return $this->rows[0] ?? null;
    }
}

final class Basket
{
    /** @use HoldsRows<Apple> */
    use HoldsRows;

    /** @param  list<Apple>  $rows */
    protected function describe(array $rows): string
    {
        return implode(', ', array_map(static fn (Apple $apple): string => $apple->name, $rows));
    }
}

// the trait's TRow stays its bound without a `@use` binding
final class Crate
{
    use HoldsRows;

    /** @param  list<object>  $rows */
    protected function describe(array $rows): string
    {
        return (string) count($rows);
    }
}

// `@use HoldsRows<Apple>` still rejects an implementation that narrows the parameter to another class
final class WrongBasket
{
    /** @use HoldsRows<Apple> */
    use HoldsRows;

    /** @param  list<Pear>  $rows */
    protected function describe(array $rows): string
    {
        return (string) count($rows);
    }
}

function probe(Basket $basket, Crate $crate): void
{
    \PHPStan\dumpType($basket->keep(new Apple('x')));
    \PHPStan\dumpType($basket->firstRow());
    \PHPStan\dumpType($crate->firstRow());
    $basket->hold([new Pear()]);
}
