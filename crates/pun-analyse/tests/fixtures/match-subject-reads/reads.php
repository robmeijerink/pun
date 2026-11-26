<?php

declare(strict_types=1);

final class Order
{
    public function __construct(public string $kind) {}
}

function find(int $id): ?Order
{
    return $id > 0 ? new Order('refund') : null;
}

function a(int $id): string
{
    $order = find($id);
    return match ($order) {
        null => 'Refund',
        default => 'Other',
    };
}

function b(int $id): string
{
    $order = find($id);
    return match ($order?->kind) {
        'refund' => 'Refund',
        default => 'Other',
    };
}

function c(int $id): string
{
    $order = find($id);
    $kind = $order?->kind;
    return match ($kind) {
        'refund' => 'Refund',
        default => 'Other',
    };
}

function d(int $id): string
{
    $order = find($id);
    return match (true) {
        $order?->kind === 'refund' => 'Refund',
        default => 'Other',
    };
}

function e(int $id): string
{
    $order = find($id);
    $x = match ($order?->kind) {
        'refund' => 'Refund',
        default => 'Other',
    };
    return $x;
}

function f(Order $order): string
{
    $o = $order;
    return match ($o->kind) {
        'refund' => 'Refund',
        default => 'Other',
    };
}

function g(int $id): string
{
    $order = find($id);
    return 'Other';
}
