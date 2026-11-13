package sa.ingenious.pdf;

import java.util.List;

/**
 * Canonical and raw document metadata. {@code documentInfo} holds every info entry, sorted by
 * key; {@code form} is {@code none}, {@code acroform} or {@code xfa}.
 */
public record Metadata(
    List<MetadataEntry> documentInfo,
    String title,
    String author,
    String subject,
    String keywords,
    String creator,
    String producer,
    String creationDateRaw,
    String creationDateIso,
    String modificationDateRaw,
    String modificationDateIso,
    PdfVersion version,
    long fileSizeBytes,
    int pageCount,
    boolean encrypted,
    PdfPermissions permissions,
    boolean linearized,
    boolean tagged,
    boolean userProperties,
    boolean suspects,
    String form,
    boolean hasJavascript,
    boolean hasMetadataStream,
    String xmpMetadata) {}
