package sa.ingenious.pdf;

public record PageSummary(int pageNumber, String text, BoundingBox bbox, double rotate) {}
