<?php

namespace TraitOverrideChain;

class Base
{
    /**
     * @param string $event
     * @param bool $halt
     */
    protected function fireModelEvent($event, $halt = true): mixed
    {
        return $halt ? $event : null;
    }
}

trait VendorOverridesModelEvents
{
    protected function fireModelEvent($event, $halt = true): mixed
    {
        return parent::fireModelEvent($event, $halt);
    }
}

class VendorInvoice extends Base
{
    use VendorOverridesModelEvents;
}

trait AppOverridesModelEvents
{
    protected function fireModelEvent($event, $halt = true): mixed
    {
        \PHPStan\dumpType($event);
        \PHPStan\dumpType($halt);

        return parent::fireModelEvent($event, $halt);
    }
}

final class AppInvoice extends VendorInvoice
{
    use AppOverridesModelEvents;
}

class Plain
{
    protected function untyped($event): mixed
    {
        return $event;
    }
}
