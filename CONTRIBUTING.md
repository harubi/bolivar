# Contributing to Bolivar

Bolivar is a Rust port of `pdfminer.six` and `pdfplumber` with Python (PyO3) and JVM (UniFFI) bindings. This guide takes a fresh clone to a green test suite.

## Prerequisites

- **Rust** — `rustup` installs the pinned toolchain from `rust-toolchain.toml` on first `cargo` call.
- **`uv`** — Python package manager: <https://docs.astral.sh/uv/>.
- **Node.js 22.14 or newer and npm** — Semantic Release validation.
- **`cargo-make`** — `cargo install cargo-make`.
- **`cargo-nextest`** — `cargo install cargo-nextest --locked`.
- **`cargo-watch`** (optional, for file-watch loops) — `cargo install cargo-watch`.

## First-time setup

```bash
git clone --recurse-submodules <repo-url>
cd bolivar
# Already cloned? Run:
git submodule update --init --depth 1
npm ci

cargo make build      # Rust workspace
cargo make build-py   # Python extension
cargo make ci         # Lint plus all tests
```

Submodules under `references/` hold upstream `pdfminer.six` and `pdfplumber` source plus sample PDFs. CI initialises them; locally you do it once.

## Available tasks

```
cargo make build         Build the Rust workspace
cargo make build-py      Build the Python extension (maturin develop)
cargo make test          Rust tests (nextest)
cargo make test-py       Python tests (depends on build-py)
cargo make test-release  Semantic Release state
cargo make test-parity   Upstream pdfminer.six and pdfplumber suites
cargo make bench         Rust benchmarks
cargo make fmt           Format Rust and Python
cargo make lint          Format check, clippy, ty, ruff, pyright, stub parity
cargo make gen-kotlin    Build UniFFI lib and generate Kotlin bindings
cargo make ci            Lint plus all tests
cargo make clean         Remove build artifacts
```

## Dev loops

| Change | Run |
|---|---|
| Rust source | `cargo make test` |
| Rust binding code (`crates/python/src/`) | `cargo make build-py && cargo make test-py` |
| Pure Python (`crates/python/python/`) | `uv run pytest tests/` — no rebuild needed |
| Anything before pushing | `cargo make ci` |

Pure-Python files in `crates/python/python/bolivar/`, `pdfminer/`, `pdfplumber/` import as ordinary modules. Only Rust changes need `maturin develop`.

## Watch loops

```bash
cargo watch -x 'nextest run --workspace'
cargo watch -x 'clippy --all-targets -- -D warnings'
```

## Single test

```bash
# Rust file
cargo nextest run -p bolivar-core --test layout_test

# Rust function
cargo nextest run -p bolivar-core --test layout_test some_test_name

# Python file
uv run pytest tests/test_foo.py

# Python function
uv run pytest tests/test_foo.py::test_one
```

## Bench tiers

```bash
cargo make bench                              # default tier
BOLIVAR_BENCH_TIER=full cargo make bench      # full tier
```

## Pre-commit hook (optional)

```bash
uv tool install pre-commit
pre-commit install
```

The hook runs `cargo fmt --check`, `cargo clippy -D warnings`, and `ruff` on staged files.

## Release

The release workflow at `.github/workflows/release.yml` is `workflow_dispatch` only and runs from `master`. semantic-release picks the version bump from conventional commit prefixes:

- `feat:` → minor
- `fix:`, `perf:`, `refactor:`, `chore:`, `docs:`, `test:`, `ci:`, `build:` → patch
- `feat!:`, `fix!:`, etc. or `BREAKING CHANGE:` footer → major

Publishing crates, wheels, and JVM artifacts is automatic from that workflow. The `cargo publish` and `maturin publish` commands stay reserved for emergency manual fallback.

## Commit and PR conventions

- Use conventional commits: `feat:`, `fix:`, `refactor:`, `perf:`, `test:`, `chore:`.
- One concern per PR.
- Mention tests run and parity or benchmark deltas when relevant.

## Repo layout

- `crates/core/` — Rust parsing, layout, and extraction engine.
- `crates/cli/` — `pdf2txt`, `dumppdf`.
- `crates/python/` — PyO3 bindings plus the `bolivar`, `pdfminer`, `pdfplumber` packages.
- `crates/uniffi/` — UniFFI bindings plus generated Java/Kotlin/Swift.
- `tests/` — Python test suite.
- `benchmarks/` — performance fixtures.
- `samples/`, `references/` — sample PDFs and upstream parity data.
