package sa.ingenious.pdf;

import java.util.List;

interface DocumentBackend {
  String extractText();

  List<PageSummary> extractPageSummaries();

  List<LayoutPage> extractLayoutPages();

  List<Table> extractTables();

  void close();
}
