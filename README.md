# bolivar

Fast PDF text and table extraction. Written in Rust, drop-in compatible with pdfminer and pdfplumber.

## Install

```sh
pip install bolivar
```

```kotlin
implementation("sa.ingenious:bolivar:1.2.0")
```

```clojure
sa.ingenious/bolivar {:mvn/version "1.2.0"}
```

```toml
[dependencies]
bolivar-core = "1.2"
```

## Extract text

Pull all text from a PDF in one call. The pdfplumber interface opens the file and iterates pages; the pdfminer interface returns the full text directly. JVM and Rust APIs follow the same pattern with their respective conventions.

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

Detect and extract tabular data from each page. Bolivar returns structured tables with row and column counts, bounding boxes, and cell text so you can inspect or export them without manual parsing.

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
try (Document doc = Document.open("doc.pdf", options)) {
    for (var table : doc.extractTables()) {
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
    for (table in it.extractTables()) {
        println("${table.rowCount}x${table.columnCount}")
    }
}
```

```clojure
(require '[sa.ingenious.pdf :as pdf])

(with-open [doc (pdf/open "doc.pdf" {:pages [1 2]})]
  (doseq [table (pdf/tables doc)]
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
try (Document doc = Document.open("doc.pdf", options)) {
    for (var page : doc.extractPageSummaries()) {
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
    for (page in it.extractPageSummaries()) {
        println("${page.pageNumber}: ${page.text.take(80)}")
    }
}
```

```clojure
(require '[sa.ingenious.pdf :as pdf])

(with-open [doc (pdf/open "doc.pdf" {:max-pages 3})]
  (doseq [page (pdf/page-summaries doc)]
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
