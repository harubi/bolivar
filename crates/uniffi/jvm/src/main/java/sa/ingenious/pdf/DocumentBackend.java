package sa.ingenious.pdf;

import java.util.List;

interface DocumentBackend {
  String extractText();

  List<PageSummary> extractPageSummaries();

  List<LayoutPage> extractLayoutPages();

  List<Table> extractTables();

  default List<Table> extractTables(TableOptions options) {
    throw new UnsupportedOperationException("extractTables(TableOptions) requires the native backend");
  }

  default List<PageTableRows> extractTableRows(TableOptions options) {
    throw new UnsupportedOperationException("extractTableRows requires the native backend");
  }

  void close();
}
