# Contributing to tudo

Thanks for your interest in tudo! It's a small project, so contributing is simple.

## Getting set up

You'll need a [Rust toolchain](https://rustup.rs/) (1.95+). Then:

```sh
git clone https://github.com/jolleydesign/tudo
cd tudo
cargo run
```

## Before you open a PR

Please make sure these all pass:

```sh
cargo test                      # unit + headless render tests
cargo clippy --all-targets      # no warnings
cargo fmt                       # formatting
```

The tests are terminal-free (they use `tempfile` and ratatui's `TestBackend`),
so they run anywhere without a real TTY.

## How the code is laid out

| Module | Responsibility |
|--------|----------------|
| `model` | core types (tasks, lists, priority) |
| `storage` | JSON read/write |
| `config` | data-directory resolution |
| `app` | state + actions |
| `event` | key/mouse mapping |
| `ui` | rendering |
| `theme` | colour palettes |

Action logic and rendering are kept terminal-free so they can be tested
directly. If you add a behaviour, add a test for it in `tests/`.

## Submitting changes

1. Fork the repo and create a branch for your change.
2. Keep commits focused and write a clear message.
3. Open a pull request describing **what** changed and **why**.
4. If you're adding a keybinding, theme, or setting, update the `README.md` too.

## Reporting bugs & ideas

Open an [issue](https://github.com/jolleydesign/tudo/issues). For bugs, please
include your OS, your terminal, and the steps to reproduce.

## License

By contributing, you agree that your contributions are licensed under the same
terms as the project.
