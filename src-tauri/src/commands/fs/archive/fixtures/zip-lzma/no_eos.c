/* Independent liblzma encoder fixture; no decoder under test is used. */
#include <lzma.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    const uint8_t input[] = "ZIP LZMA without an end marker.\n";
    const size_t size = argc > 1 && strcmp(argv[1], "empty") == 0 ? 0 : sizeof(input) - 1;
    lzma_options_lzma options;
    if (lzma_lzma_preset(&options, 1)) return 1;
    options.ext_flags = 0;
    lzma_filter filters[] = {{LZMA_FILTER_LZMA1EXT, &options}, {LZMA_VLI_UNKNOWN, NULL}};
    uint8_t output[4096];
    size_t position = 0;
    if (lzma_raw_buffer_encode(filters, NULL, input, size, output, &position, sizeof(output)) != LZMA_OK) return 2;
    lzma_filter properties_filter = {LZMA_FILTER_LZMA1, &options};
    uint8_t properties[5];
    if (lzma_properties_encode(&properties_filter, properties) != LZMA_OK) return 3;
    if (fwrite(properties, 1, sizeof(properties), stdout) != sizeof(properties)) return 4;
    return fwrite(output, 1, position, stdout) == position ? 0 : 5;
}
