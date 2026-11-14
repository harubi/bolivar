# bolivar

Fast PDF text and table extraction. Written in Rust, drop-in compatible with pdfminer and pdfplumber.

## Install

```sh
pip install bolivar
```

```kotlin
// build.gradle.kts
implementation("sa.ingenious:bolivar:<version>")
```

```clojure
;; deps.edn
sa.ingenious/bolivar {:mvn/version "<version>"}
```

```toml
[dependencies]
bolivar-core = "<version>"
```

## JVM requirements

The JVM library needs **Java 25 or newer** and calls its native library through Java's Foreign Function & Memory API. Allow native access when you launch the JVM:

```sh
java --enable-native-access=ALL-UNNAMED -cp app.jar com.example.Main
```

On the module path, name the module instead: `--enable-native-access=sa.ingenious.bolivar`. An executable jar can carry `Enable-Native-Access: ALL-UNNAMED` in its manifest and skip the flag for `java -jar`.

```clojure
;; deps.edn
{:aliases {:dev {:jvm-opts ["--enable-native-access=ALL-UNNAMED"]}}}

;; project.clj (Leiningen)
:jvm-opts ["--enable-native-access=ALL-UNNAMED"]
```

```kotlin
// build.gradle.kts
application { applicationDefaultJvmArgs = listOf("--enable-native-access=ALL-UNNAMED") }
tasks.test { jvmArgs("--enable-native-access=ALL-UNNAMED") }
```

```xml
<!-- Maven Surefire -->
<argLine>--enable-native-access=ALL-UNNAMED</argLine>
```

Without the flag, Java 25 still runs Bolivar but prints this warning on first use, and the JDK states that a future release will block the call:

```text
WARNING: A restricted method in java.lang.foreign.SymbolLookup has been called
WARNING: java.lang.foreign.SymbolLookup::libraryLookup has been called by sa.ingenious.pdf.internal.NativeLoader in an unnamed module
WARNING: Use --enable-native-access=ALL-UNNAMED to avoid a warning for callers in this module
WARNING: Restricted methods will be blocked in a future release unless native access is enabled
```

The jar carries native libraries for Linux (x86-64, aarch64, glibc), macOS (x86-64, aarch64) and Windows (x86-64). It extracts the one for your platform to `${java.io.tmpdir}/bolivar-<user>/`, checks its SHA-256 and reuses it on later runs. Set `-Dbolivar.cache.dir=<dir>` when the temp directory is mounted `noexec`, or `-Dbolivar.library.path=<file>` to load a library you built. Alpine and other musl systems are not supported.

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
import sa.ingenious.pdf.LayoutOptions;
import sa.ingenious.pdf.Options;

var options = Options.builder()
    .maxPages(1)
    .layout(LayoutOptions.builder().lineMargin(0.5).wordMargin(0.1).build())
    .build();

String text = Document.extractText("doc.pdf", options);
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

Detect and extract tabular data from each page. In Java, table APIs return a closeable, single-pass cursor; close it to stop work early and release its native state. In Clojure, they return a reducible that opens and closes its cursor inside each reduction.

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
import sa.ingenious.pdf.Options;

var options = Options.builder().pages(1, 2).build();
try (Document doc = Document.open("doc.pdf", options);
     var tables = doc.tables()) {
    for (var table : tables) {
        System.out.println(table.rowCount() + "x" + table.columnCount());
    }
}
```

```clojure
(require '[sa.ingenious.pdf :as pdf])

(with-open [doc (pdf/open "doc.pdf" {:pages [1 2]})]
  (run! #(println (:row-count %) "x" (:column-count %))
        (pdf/tables doc {:vertical-strategy :text})))
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
import sa.ingenious.pdf.Options;

var options = Options.builder().maxPages(3).build();
try (Document doc = Document.open("doc.pdf", options);
     var pages = doc.pageSummaries()) {
    for (var page : pages) {
        System.out.println(page.pageNumber() + ": " + page.text().substring(0, Math.min(80, page.text().length())));
    }
}
```

```clojure
(require '[sa.ingenious.pdf :as pdf])

(with-open [doc (pdf/open "doc.pdf" {:max-pages 3})]
  (run! (fn [{:keys [page-number text]}]
          (println page-number (subs text 0 (min 80 (count text)))))
        (pdf/page-summaries doc)))
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

### Choose the pages each cursor reads (JVM)

Every cursor takes its own options. They narrow the pages the document's options select, never widen them:

- `pages`: the 1-based page numbers to keep. An empty list keeps none.
- `skip`: how many of those pages to pass over first.
- `maxPages`: the most pages to read.
- `readAhead`: how many pages may be started or finished but not yet read, ahead of the reader. The default is 50. It also caps the cursor's parallelism and how many finished pages wait in memory behind a slow one.

Pages come in document order, each once. A cursor that ends early still pays for the pages it started ahead of you. When you know you need only a few pages, select them: `maxPages(2)` reads two pages, where stopping after two pages of a full cursor can cost most of the document. A first cursor with `maxPages(2)` and a second with `skip(2)` read every selected page exactly once:

```java
var head = CursorOptions.builder().maxPages(2).build();
var rest = CursorOptions.builder().skip(2).build();
try (Document doc = Document.open("doc.pdf");
     var first = doc.pageTexts(head)) {
    first.forEachRemaining(page -> System.out.println(page.pageNumber()));
    try (var others = doc.pageTexts(rest)) {
        others.forEachRemaining(page -> System.out.println(page.pageNumber()));
    }
}
```

```clojure
(with-open [doc (pdf/open "doc.pdf")]
  (let [head (into [] (pdf/page-texts doc {:max-pages 2}))]
    (when (looks-right? head)
      (into head (pdf/page-texts doc {:skip 2})))))
```

`tables` and `table-rows` take the same keys in their options map.

## Layout and characters (JVM)

Layout and raw pages carry every text box, line and character. The JVM library holds each page as columns and frees the native copy as soon as the page is read, so a 4,000-character page costs a handful of arrays, not 4,000 objects.

In Java, a page hands out views (`TextBox`, `TextLine`, `Glyph`) that read its columns in place. Views keep their page alive; `toRecord()` and `toRecords()` copy into records that do not, and the `glyph...s()` methods copy whole columns into arrays:

```java
try (var doc = Document.open("doc.pdf");
     var pages = doc.pages()) {
    for (LayoutPage page : pages) {
        for (TextBox box : page.textBoxes()) {
            for (TextLine line : box.lines()) {
                for (Glyph glyph : line.glyphs()) {
                    if (glyph.x0() > 100) {
                        System.out.println(glyph.fontName() + " " + glyph.text());
                    }
                }
            }
        }
        double[] x0 = page.glyphX0s();               // one column, copied
        List<LayoutTextBox> kept = page.toRecords(); // detached from the page
    }
}
```

In Clojure, pages are map-like views: `(:chars page)`, `(:x0 char)` and `(:font-name char)` read the columns directly. A view keeps its whole page in memory, so `realize` what you keep:

```clojure
(with-open [doc (pdf/open "doc.pdf")]
  ;; Count the characters right of x = 100, per font, without building a map per character.
  (transduce (comp (mapcat :chars)
                   (filter #(> (:x0 %) 100))
                   (map :font-name))
             (completing #(update %1 %2 (fnil inc 0)))
             {}
             (pdf/pages doc)))

(with-open [doc (pdf/open "doc.pdf")]
  ;; Keep plain maps, not views, past the reduction.
  (into [] (comp (mapcat :lines) (take 5) (map pdf/realize)) (pdf/pages doc)))
```

`pdf/columns` returns a page's characters as primitive arrays for vectorised work.

### Moving from 2.x

| 2.x | 3.x |
|---|---|
| `DocumentOptions`, Kotlin DSL | `Options` and `LayoutOptions` builders |
| `Document.extractLayoutPages()` | `Document.pages()` cursor of `LayoutPage` |
| `Document.extractRawDocument()` | `Document.rawPages()` cursor of `RawPage` |
| `*Async` methods, Kotlin `suspend` helpers | Removed: call from a virtual thread |
| `TableOptions` strategy strings | `TableStrategy` enum |
| `pdf/layout-pages`, `pdf/raw-document` | `pdf/pages`, `pdf/raw-pages` (reducible views) |
| `(with-open [c (pdf/tables doc)] (doseq ...))` | `(run! f (pdf/tables doc))`; sources are not seqs |
| `:boxes-flow nil` meant the 0.5 default | `:boxes-flow nil` turns the ordering off, as in pdfminer |
| ex-info `:type :pdf/native-error` | ex-info with `:cognitect.anomalies/category`, `:sa.ingenious.pdf/error` (such as `:io-not-found`), `:sa.ingenious.pdf/code` and `:sa.ingenious.pdf/key` (the bad option) |
| `LayoutPage.charX0(i)`, `charText(i)` | `page.glyphs()` views (`Glyph.x0()`, `text()`), or `glyphX0s()` columns |

## Async (Python)

Run extraction off the main thread in Python while keeping the same `pdfplumber` API.

```python
import pdfplumber

async with pdfplumber.open("doc.pdf") as pdf:
    for page in pdf.pages:
        for table in page.extract_tables():
            print(table)
```

`extract_pages_async` streams laid-out pages. `read_ahead` bounds how many pages run ahead of the reader (default 50), and with it the parallelism and the pages held in memory:

```python
from bolivar import extract_pages_async

async for page in extract_pages_async(data, read_ahead=4):
    print(page.pageid)
```

## License

MIT
