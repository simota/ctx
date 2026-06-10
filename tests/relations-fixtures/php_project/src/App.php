<?php

namespace App;

use App\Foo;
use App\Bar;

class App {
    public function go(): string {
        return (new Foo())->hello() . ' ' . (new Bar())->world();
    }
}
