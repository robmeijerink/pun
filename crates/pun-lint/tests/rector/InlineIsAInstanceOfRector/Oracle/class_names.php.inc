<?php

namespace App;

class Open {
    const NAME = 'App\Open';
    const LOWER = 'app\open';
    const NOPE = 'App\Nope';
    public function foo() {}
    public function bar(): mixed {}
}

trait T
{
    public function t(object $o)
    {
        $a = is_a($o, self::class);
        $b = is_a($o, static::class);
    }
}

final class C extends Unknown
{
    public function run(object $o, Open $open, ?Open $n)
    {
        $a = is_a($o, $this::class);
        $b = is_a($o, Open::NAME);
        $c = is_a($o, Open::LOWER);
        $d = is_a($o, Open::NOPE);
        $e = is_a($o, parent::class);
        $f = is_a($open->foo(), Open::class);
        $g = is_a($open->bar(), Open::class);
        $h = is_a($n ?? $open, Open::class);
        $i = is_a($open ?? $n, Open::class);
        $j = is_a($open ?? $open, Open::class);
        $k = is_a($o, static::class);
        $l = -is_a($o, static::class);
        $m = @is_a($o, static::class);
        $p = is_a($o, \App\Open::class);
        $q = is_a($o, namespace\Open::class);
        $r = is_a($n?->foo(), Open::class);
        $s = is_a($open::make(), Open::class);
        $t = is_a($o, Open::class,);
        $u = is_a(
            $o,
            Open::class
        );
        $v = is_a(/* c */ $o, Open::class);
        $w = is_a($o, \stdclass::class);
    }
}
-----
<?php

namespace App;

class Open {
    const NAME = 'App\Open';
    const LOWER = 'app\open';
    const NOPE = 'App\Nope';
    public function foo() {}
    public function bar(): mixed {}
}

trait T
{
    public function t(object $o)
    {
        $a = $o instanceof self;
        $b = $o instanceof static;
    }
}

final class C extends Unknown
{
    public function run(object $o, Open $open, ?Open $n)
    {
        $a = $o instanceof \App\C;
        $b = $o instanceof \App\Open;
        $c = is_a($o, Open::LOWER);
        $d = is_a($o, Open::NOPE);
        $e = is_a($o, parent::class);
        $f = is_a($open->foo(), Open::class);
        $g = is_a($open->bar(), Open::class);
        $h = ($n ?? $open) instanceof Open;
        $i = ($open ?? $n) instanceof Open;
        $j = is_a($open ?? $open, Open::class);
        $k = $o instanceof static;
        $l = -($o instanceof static);
        $m = @($o instanceof static);
        $p = $o instanceof \App\Open;
        $q = $o instanceof namespace\Open;
        $r = is_a($n?->foo(), Open::class);
        $s = is_a($open::make(), Open::class);
        $t = $o instanceof Open;
        $u = $o instanceof Open;
        $v = $o instanceof Open;
        $w = is_a($o, \stdclass::class);
    }
}
