#!/usr/bin/env python3
"""Writes crates/pun-config/rules/rector-sets.json: every Rector set (Rector 2.9.2, rector-laravel, rector-phpunit and
the Pest plugin) under its Pun name, with the Pun lint rules its Rector rules map to and, for each, the Rector rules
(short class names) of the set behind it. The set membership comes from the upstream exports in
crates/pun-config/rules/upstream/ (each rule's `sets`); the Pun rule from mapping.toml's [upstream.*] entries. Rector
rules that are format rules in Pun, or unsupported, are left out. `rector/php-sets` (`withPhpSets()`) is not listed:
pun-config builds it from the `rector/phpNN` sets up to `[php] version`.

Pun names: `rector/<set>` for Rector core's sets, `rector-laravel/<set>`, `rector-phpunit/<set>` and
`pest/<set>` (without its `pest-` prefix). Run with --check to fail when the file is stale."""
import json, os, sys, tomllib

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
rules = os.path.join(root, 'crates/pun-config/rules')
mapping = tomllib.load(open(os.path.join(rules, 'mapping.toml'), 'rb'))
LARAVEL = ('laravel-', 'up-to-laravel-', 'livewire-', 'up-to-livewire-', 'cashier-', 'up-to-cashier-', 'faker-')


def pun_name(s):
    if s.startswith('phpunit:'):
        return 'rector-phpunit/' + s[len('phpunit:'):]
    if s.startswith(LARAVEL) or s == 'lumen':
        return 'rector-laravel/' + s
    if s.startswith('pest-'):
        return 'pest/' + s[len('pest-'):]
    return 'rector/' + s


by_class = {}
for tool in ('rector', 'rector-laravel', 'rector-phpunit', 'pest-plugin-rector'):
    for e in mapping['upstream'][tool].values():
        if e.get('class') and e.get('pun', '').startswith('lint/'):
            by_class[e['class']] = e['pun']

sets = {}
for f in ('rector-core.json', 'rector-ecosystem.json'):
    for row in json.load(open(os.path.join(rules, 'upstream', f))):
        pun = by_class.get(row['fqcn'])
        if pun is None:
            continue
        short = row['fqcn'].rsplit('\\', 1)[1]
        for s in row['sets']:
            members = sets.setdefault(pun_name(s), {})
            if short not in members.setdefault(pun, []):
                members[pun].append(short)

out = {s: {r: sorted(c) for r, c in sorted(m.items())} for s, m in sorted(sets.items())}
text = json.dumps(out, indent=1, sort_keys=True) + '\n'
path = os.path.join(rules, 'rector-sets.json')
if '--check' in sys.argv:
    if open(path).read() != text:
        sys.exit(f'{path} is stale: run scripts/gen-rector-sets.py')
    print(f'{len(out)} Rector sets; in sync')
else:
    open(path, 'w').write(text)
    print(f'{len(out)} Rector sets written')
