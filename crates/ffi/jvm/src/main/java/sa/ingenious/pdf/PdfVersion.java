package sa.ingenious.pdf;

/** The version in the file header, in the catalog, and the one in effect. */
public record PdfVersion(String header, String catalog, String effective) {}
