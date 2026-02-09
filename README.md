# Bolivar

Fast PDF content extraction, written in Rust with Python bindings.

Bolivar is a from-scratch Rust port of [pdfminer.six](https://github.com/pdfminer/pdfminer.six) and [pdfplumber](https://github.com/jsvine/pdfplumber). It aims to be a drop-in replacement: swap `pdfminer`/`pdfplumber` for `bolivar` and keep your existing code working.

## Install

**Python:**

```bash
pip install bolivar
```

**Rust:**

```bash
cargo add bolivar-core
```

## Usage

### Python (pdfplumber-compatible)

```python
import pdfplumber

with pdfplumber.open("example.pdf") as pdf:
    for page in pdf.pages:
        print(page.extract_text())
```

### Rust

```rust
use bolivar_core::high_level::extract_text;

fn main() {
    let text = extract_text("example.pdf").unwrap();
    println!("{text}");
}
```

## License

[MIT](LICENSE)
