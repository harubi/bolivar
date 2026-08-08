package sa.ingenious.pdf;

import java.util.List;

interface DocumentBackend {
  String extractText();

  List<PageSummary> extractPageSummaries();

  List<LayoutPage> extractLayoutPages();

  RawDocument extractRawDocument();

  default RawPage extractRawPage(int pageNumber) {
    throw new UnsupportedOperationException("extractRawPage requires the native backend");
  }

  default RawDocumentMetadata metadata() {
    throw new UnsupportedOperationException("metadata requires the native backend");
  }

  List<Table> extractTables();

  default List<Table> extractTables(TableOptions options) {
    throw new UnsupportedOperationException("extractTables(TableOptions) requires the native backend");
  }

  default List<PageTableRows> extractTableRows(TableOptions options) {
    throw new UnsupportedOperationException("extractTableRows requires the native backend");
  }

  void close();
}
