# bolivar

Fast PDF text and table extraction. Written in Rust, drop-in compatible with pdfminer and pdfplumber.

## Install

```sh
pip install bolivar
```

```kotlin
implementation("sa.ingenious:bolivar:1.2.0")
```

```toml
[dependencies]
bolivar-core = "1.2"
```

## Extract text

Pull all text from a PDF in one call. The pdfplumber interface opens the file and iterates pages; the pdfminer interface returns the full text directly. Kotlin and Rust follow the same pattern with their respective APIs.

```python
import pdfplumber

with pdfplumber.open("doc.pdf") as pdf:
    for page in pdf.pages:
        print(page.extract_text())
```

```python
from pdfminer.high_level import extract_text

text = extract_text("doc.pdf")
```

```kotlin
import sa.ingenious.bolivar.Bolivar
import sa.ingenious.bolivar.DocumentOptions

val doc = Bolivar.open("doc.pdf", DocumentOptions {
    maxPages = 1
    layout {
        lineMargin = 0.5
        wordMargin = 0.1
    }
})
val text = doc.extractText()
```

```rust
use bolivar_core::high_level::extract_text;

fn main() -> bolivar_core::Result<()> {
    let data = std::fs::read("doc.pdf")?;
    let text = extract_text(&data, None)?;
    println!("{text}");
    Ok(())
}
```

## Extract tables

Detect and extract tabular data from each page. Bolivar returns structured tables with row and column counts, bounding boxes, and cell text so you can inspect or export them without manual parsing.

```python
import pdfplumber

with pdfplumber.open("doc.pdf") as pdf:
    for page in pdf.pages:
        for table in page.extract_tables():
            print(table)
```

```kotlin
import sa.ingenious.bolivar.Bolivar
import sa.ingenious.bolivar.DocumentOptions

val doc = Bolivar.open("doc.pdf", DocumentOptions {
    pages(1, 2)
})
val tables = doc.extractTables()
for (table in tables) {
    println("${table.rowCount}x${table.columnCount}")
}
```

```rust
use bolivar_core::high_level::{extract_tables_with_document, ExtractOptions};
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::table::TableSettings;

fn main() -> bolivar_core::Result<()> {
    let data = std::fs::read("doc.pdf")?;
    let doc = PDFDocument::new(&data, "")?;
    let tables = extract_tables_with_document(
        &doc,
        ExtractOptions::default(),
        &TableSettings::default(),
    )?;
    Ok(())
}
```

## Iterate pages

Walk through pages one at a time to read metadata like page number, dimensions, and a text preview. This is useful when you need to locate content across a large document before extracting specific pages.

```python
import pdfplumber

with pdfplumber.open("doc.pdf") as pdf:
    for page in pdf.pages:
        print(page.page_number, page.width, page.height)
```

```python
from pdfminer.high_level import extract_pages

for page in extract_pages("doc.pdf"):
    print(page.pageid, page.width, page.height)
```

```kotlin
import sa.ingenious.bolivar.Bolivar
import sa.ingenious.bolivar.DocumentOptions

val doc = Bolivar.open("doc.pdf", DocumentOptions {
    maxPages = 3
})
val pages = doc.extractPageSummaries()
for (page in pages) {
    println("${page.pageNumber}: ${page.text.take(80)}")
}
```

```rust
use bolivar_core::high_level::extract_pages;

fn main() -> bolivar_core::Result<()> {
    let data = std::fs::read("doc.pdf")?;
    for page in extract_pages(&data, None)? {
        let page = page?;
        println!("{}", page.pageid);
    }
    Ok(())
}
```

## Async

Run extraction off the main thread. Kotlin offers both coroutine suspend functions and Java-compatible `CompletableFuture` so you can choose whichever fits your concurrency model.

```python
import pdfplumber

async with pdfplumber.open("doc.pdf") as pdf:
    for page in pdf.pages:
        for table in page.extract_tables():
            print(table)
```

```kotlin
import kotlinx.coroutines.runBlocking
import sa.ingenious.bolivar.Bolivar
import sa.ingenious.bolivar.DocumentOptions

runBlocking {
    val doc = Bolivar.open("doc.pdf", DocumentOptions {
        pages(1, 2)
    })
    val tables = doc.extractTablesAsync()
    println(tables.size)
}
```

```kotlin
import sa.ingenious.bolivar.Bolivar
import sa.ingenious.bolivar.DocumentOptions
import sa.ingenious.bolivar.Table
import java.util.concurrent.CompletableFuture

val doc = Bolivar.open("doc.pdf", DocumentOptions {
    pages(1, 2)
})
val future: CompletableFuture<List<Table>> = doc.extractTablesFuture()
val tables = future.get()
```

## License

MIT
