package sa.ingenious.pdf;

import org.jspecify.annotations.Nullable;

/**
 * The version in the file header, in the catalog, and the one in effect: the catalog's when it
 * names one, else the header's.
 */
public record PdfVersion(
    @Nullable String header, @Nullable String catalog, @Nullable String effective) {}
