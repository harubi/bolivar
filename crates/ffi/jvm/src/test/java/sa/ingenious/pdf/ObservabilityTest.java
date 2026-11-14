package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import jdk.jfr.Recording;
import jdk.jfr.consumer.RecordedEvent;
import jdk.jfr.consumer.RecordingFile;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** Flight Recorder sees opens and batches. */
class ObservabilityTest {
  @Test
  void opensAndBatchesAreRecorded(@TempDir Path dir) throws Exception {
    Path file = dir.resolve("bolivar.jfr");
    try (var recording = new Recording()) {
      recording.enable("sa.ingenious.bolivar.Open");
      recording.enable("sa.ingenious.bolivar.Batch");
      recording.start();
      try (var document = Document.open(Fixtures.path("contrib/pagelabels.pdf"));
          var pages = document.pages()) {
        pages.forEachRemaining(page -> {});
        document.text();
      }
      recording.stop();
      recording.dump(file);
    }
    Map<String, Integer> counts = new HashMap<>();
    for (RecordedEvent event : RecordingFile.readAllEvents(file)) {
      String key = event.getEventType().getName();
      if (key.endsWith("Batch")) {
        key += ":" + event.getString("kind");
        assertTrue(event.getLong("elements") > 0);
      }
      counts.merge(key, 1, Integer::sum);
    }
    assertEquals(1, counts.get("sa.ingenious.bolivar.Open"));
    assertEquals(5, counts.get("sa.ingenious.bolivar.Batch:LAYOUT"));
    assertEquals(1, counts.get("sa.ingenious.bolivar.Batch:TEXT"));
    assertTrue(Files.size(file) > 0);
  }
}
