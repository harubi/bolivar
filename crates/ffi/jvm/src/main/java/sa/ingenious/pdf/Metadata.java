package sa.ingenious.pdf;

import java.util.List;
import org.jspecify.annotations.Nullable;

/**
 * Canonical and raw document metadata. {@code documentInfo} holds every info entry, sorted by key;
 * {@code form} is {@code none}, {@code acroform} or {@code xfa}.
 */
public record Metadata(
    List<MetadataEntry> documentInfo,
    @Nullable String title,
    @Nullable String author,
    @Nullable String subject,
    @Nullable String keywords,
    @Nullable String creator,
    @Nullable String producer,
    @Nullable String creationDateRaw,
    @Nullable String creationDateIso,
    @Nullable String modificationDateRaw,
    @Nullable String modificationDateIso,
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
    @Nullable String xmpMetadata) {}
