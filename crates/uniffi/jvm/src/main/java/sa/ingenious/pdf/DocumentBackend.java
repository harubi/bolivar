package sa.ingenious.pdf;

import java.util.List;

interface DocumentBackend {
  String extractText();

  CursorBackend<PageSummary> pageSummaries();

  default CursorBackend<PageText> pageTexts() {
    throw new UnsupportedOperationException("pageTexts requires the native backend");
  }

  List<LayoutPage> extractLayoutPages();

  RawDocument extractRawDocument();

  default RawPage extractRawPage(int pageNumber) {
    throw new UnsupportedOperationException("extractRawPage requires the native backend");
  }

  default RawDocumentMetadata metadata() {
    throw new UnsupportedOperationException("metadata requires the native backend");
  }

  CursorBackend<Table> tables(TableOptions options);

  CursorBackend<PageTableRows> tableRows(TableOptions options);

  void close();
}
