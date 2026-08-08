#define U_STATIC_IMPLEMENTATION

#include <stdint.h>
#include <unicode/ubidi.h>
#include <unicode/utypes.h>

void *bolivar_icu_bidi_open(void) {
    return ubidi_open();
}

void bolivar_icu_bidi_close(void *bidi) {
    ubidi_close((UBiDi *)bidi);
}

int32_t bolivar_icu_bidi_inverse(
    void *bidi,
    const uint16_t *source,
    int32_t source_length,
    uint8_t paragraph_level,
    uint16_t *destination,
    int32_t destination_capacity,
    int32_t *output_to_source
) {
    UErrorCode status = U_ZERO_ERROR;
    UBiDi *context = (UBiDi *)bidi;

    ubidi_setReorderingMode(context, UBIDI_REORDER_INVERSE_LIKE_DIRECT);
    ubidi_setPara(
        context,
        (const UChar *)source,
        source_length,
        paragraph_level,
        NULL,
        &status
    );
    if (U_FAILURE(status)) {
        return -(int32_t)status;
    }

    int32_t output_length = ubidi_writeReordered(
        context,
        (UChar *)destination,
        destination_capacity,
        0,
        &status
    );
    if (U_FAILURE(status)) {
        return -(int32_t)status;
    }

    if (output_to_source != NULL) {
        ubidi_getVisualMap(context, output_to_source, &status);
        if (U_FAILURE(status)) {
            return -(int32_t)status;
        }
    }

    return output_length;
}
