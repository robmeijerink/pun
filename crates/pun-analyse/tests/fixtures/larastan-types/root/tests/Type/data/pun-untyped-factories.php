<?php

namespace PunUntypedFactories;

use Illuminate\Database\Eloquent\Factories\Factory;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;

use function PHPStan\Testing\assertType;

/** @property string $status */
class Order extends Model
{
    /** @phpstan-ignore missingType.generics */
    use HasFactory;

    /** @phpstan-ignore missingType.return */
    protected static function newFactory()
    {
        return OrderFactory::new();
    }
}

/** @property string $status */
class Invoice extends Model
{
    /** @phpstan-ignore missingType.generics */
    use HasFactory;

    protected static function newFactory(): InvoiceFactory
    {
        return InvoiceFactory::new();
    }
}

/** @phpstan-ignore missingType.generics */
class OrderFactory extends Factory
{
    protected $model = Order::class;

    public function definition(): array
    {
        return ['status' => 'new'];
    }
}

/** @phpstan-ignore missingType.generics */
class InvoiceFactory extends Factory
{
    protected $model = Invoice::class;

    public function definition(): array
    {
        return ['status' => 'new'];
    }
}

function status(): string
{
    $order = Order::factory()->create(['status' => 'processing']);
    assertType('mixed', $order);

    return $order->status;
}

function invoice(): string
{
    $invoice = Invoice::factory()->create(['status' => 'processing']);
    assertType('PunUntypedFactories\InvoiceFactory', Invoice::factory());
    assertType('Illuminate\Database\Eloquent\Model', $invoice);

    return $invoice->status;
}
