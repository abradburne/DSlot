# Contributing

Bug reports and changes are welcome. [docs/architecture.md](docs/architecture.md) is the map of
the code and [docs/building.md](docs/building.md) says how to build, test and run the desktop
preview; run the tests for both builds (`--features nibos` and `--features stock`) before
sending a change.

## Signing off (DCO)

Every commit carries a `Signed-off-by:` line with your real name and email, saying you wrote it
or have the right to submit it under this licence: the
[Developer Certificate of Origin](https://developercertificate.org/). `git commit -s` adds it;
CI checks every commit of a pull request for one.

## Licence

DSlot is GPL-3.0-or-later ([LICENSE](LICENSE)). By contributing you agree that your contribution
is licensed under the same terms. Keep a `GPL-3.0-or-later` licence on anything new.

A new dependency must be under a licence GPL-3.0 can take (MIT, Apache-2.0, BSD, Zlib, ISC,
MPL-2.0, LGPL or GPL-3.0 and the like; not GPL-2.0-only, and nothing non-free). After changing
`Cargo.lock`, regenerate the notices and check them ([docs/licences.md](docs/licences.md)):

```bash
python3 tools/third_party.py > THIRD-PARTY.md
```

Nothing from Nintendo, no game artwork, ROMs, BIOS files or emulator code, and no photograph
that is not freely licensed and credited ([docs/system-cards.md](docs/system-cards.md)).
