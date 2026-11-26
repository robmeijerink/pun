<?php

namespace PunTableAttribute;

use Illuminate\Database\Eloquent\Attributes\Table;
use Illuminate\Database\Eloquent\Model;

use function PHPStan\Testing\assertType;

#[Table(name: 'users')]
class Item extends Model
{
}

final class Product extends Item
{
}

#[Table('users')]
final class Positional extends Model
{
}

#[Table(name: 'no_such_table')]
final class Declared extends Model
{
    /** @var string */
    protected $table = 'users';
}

final class Usage
{
    public function columns(Item $item, Product $product, Positional $positional, Declared $declared): void
    {
        assertType('string', $item->name);
        assertType('string', $product->name);
        assertType('string', $positional->email);
        assertType('string', $declared->name);
    }

    public function missing(Item $item): mixed
    {
        return $item->missing;
    }
}
