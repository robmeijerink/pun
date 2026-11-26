<?php

namespace App;

class Open
{
}

function run(object $o)
{
    return is_a($o = new Open(), Open::class);
}
-----
<?php

namespace App;

class Open
{
}

function run(object $o)
{
    return ($o = new Open()) instanceof Open;
}
