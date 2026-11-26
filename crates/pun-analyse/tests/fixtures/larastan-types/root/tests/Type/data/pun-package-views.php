<?php

namespace PunPackageViews;

final class Views
{
    public function head(): string
    {
        return view('punacme::head')->render();
    }

    public function nested(): string
    {
        return view('punacme::mail.footer')->render();
    }

    public function missing(): string
    {
        return view('punacme::missing')->render();
    }

    public function unknown(): string
    {
        return view('other::head')->render();
    }
}
