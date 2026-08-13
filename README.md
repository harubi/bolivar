# bolivar

Fast PDF text and table extraction. Written in Rust, drop-in compatible with pdfminer and pdfplumber.

## Install

```sh
pip install bolivar
```

```kotlin
implementation("sa.ingenious:bolivar:<version>")
```

```clojure
sa.ingenious/bolivar {:mvn/version "<version>"}
```

```toml
[dependencies]
bolivar-core = "<version>"
```

## Extract text

Pull all text from a PDF in one call. The pdfplumber interface opens the file and iterates pages; the pdfminer interface returns the full text directly. JVM and Rust APIs follow the same pattern with their respective conventions.

Path APIs memory-map the source file. Do not change or replace the file until the operation ends and all documents or cursors that use it are released.

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

```java
import sa.ingenious.pdf.Document;
import sa.ingenious.pdf.DocumentOptions;

var options = DocumentOptions.builder()
    .maxPages(1)
    .layout(layout -> layout.lineMargin(0.5).wordMargin(0.1))
    .build();

String text = Document.extractText("doc.pdf", options);
```

```kotlin
import sa.ingenious.pdf.extractText

val text = extractText("doc.pdf") {
    maxPages = 1
    layout {
        lineMargin = 0.5
        wordMargin = 0.1
    }
}
```

```clojure
(require '[sa.ingenious.pdf :as pdf])

(def text (pdf/extract-text "doc.pdf"))
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

Detect and extract tabular data from each page. JVM table APIs return a closeable, single-pass cursor. Close the cursor to stop work early and release its native state.

Each open cursor has a fixed 50-page window. Those pages include active work and completed results that wait for an earlier page. Several open cursors have separate bounded windows. Close signals cooperative cancellation; active page work stops at its next checkpoint.

```python
import pdfplumber

with pdfplumber.open("doc.pdf") as pdf:
    for page in pdf.pages:
        for table in page.extract_tables():
            print(table)
```

```java
import sa.ingenious.pdf.Document;
import sa.ingenious.pdf.DocumentOptions;

var options = DocumentOptions.builder().pages(1, 2).build();
try (Document doc = Document.open("doc.pdf", options);
     var tables = doc.tables()) {
    for (var table : tables) {
        System.out.println(table.rowCount() + "x" + table.columnCount());
    }
}
```

```kotlin
import sa.ingenious.pdf.openDocument

val doc = openDocument("doc.pdf") {
    pages(1, 2)
}
doc.use {
    it.tables().use { tables ->
        for (table in tables) {
            println("${table.rowCount}x${table.columnCount}")
        }
    }
}
```

```clojure
(require '[sa.ingenious.pdf :as pdf])

(with-open [doc (pdf/open "doc.pdf" {:pages [1 2]})
            tables (pdf/tables doc)]
  (doseq [table tables]
    (println (:row-count table) "x" (:column-count table))))
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

```java
import sa.ingenious.pdf.Document;
import sa.ingenious.pdf.DocumentOptions;

var options = DocumentOptions.builder().maxPages(3).build();
try (Document doc = Document.open("doc.pdf", options);
     var pages = doc.pageSummaries()) {
    for (var page : pages) {
        System.out.println(page.pageNumber() + ": " + page.text().substring(0, Math.min(80, page.text().length())));
    }
}
```

```kotlin
import sa.ingenious.pdf.openDocument

val doc = openDocument("doc.pdf") {
    maxPages = 3
}
doc.use {
    it.pageSummaries().use { pages ->
        for (page in pages) {
            println("${page.pageNumber}: ${page.text.take(80)}")
        }
    }
}
```

```clojure
(require '[sa.ingenious.pdf :as pdf])

(with-open [doc (pdf/open "doc.pdf" {:max-pages 3})
            pages (pdf/page-summaries doc)]
  (doseq [page pages]
    (println (:page-number page) (subs (:text page) 0 (min 80 (count (:text page)))))))
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

## Async (Python)

Run extraction off the main thread in Python while keeping the same `pdfplumber` API.

```python
import pdfplumber

async with pdfplumber.open("doc.pdf") as pdf:
    for page in pdf.pages:
        for table in page.extract_tables():
            print(table)
```

## License

MIT
