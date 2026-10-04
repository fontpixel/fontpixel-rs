/* Original MIT test adapter. Native parsing/addressing is unmodified font.c. */
#include "font.c"
struct font sysfont;
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    int fd = open(argv[1], O_RDONLY);
    if (fd < 0) return 3;
    struct font temporary = {0}, font = {0};
    uint32_t widths, offsets;
    if (!font_load_header(fd, &temporary, &font, &widths, &offsets)) return 4;
    int size = ffilesize(fd);
    if (size < 0) return 5;
    unsigned char *bytes = malloc((size_t)size + 4);
    if (!bytes) return 5;
    lseek(fd, 0, SEEK_SET);
    if (read(fd, bytes, (size_t)size) != size) return 5;
    close(fd);
    font.buffer_start = bytes;
    font.buffer_end = bytes + size;
    font.buffer_size = size;
    font.fd = -1;
    font.disabled = false;
    if (!font_load_in_memory(&font, widths, offsets)) return 6;
    printf("{\"ascent\":%d,\"height\":%u,\"glyphs\":[", font.ascent, font.height);
    for (int i = 0; i < font.size; i++) {
        ucschar_t cp = font.firstchar + i;
        int width = font_get_width(&font, cp);
        const unsigned char *p = font_get_bits(&font, cp);
        if (i) putchar(',');
        printf("{\"cp\":%u,\"width\":%d,\"pixels\":[", cp, width);
        for (unsigned y = 0; y < font.height; y++) {
            for (int x = 0; x < width; x++) {
                if (y || x) putchar(',');
                unsigned k = y * width + x;
                unsigned value = font.depth
                    ? 255 - ((p[k / 2] >> ((k % 2) * 4)) & 15) * 17
                    : (p[(y / 8) * width + x] & (1 << (y % 8))) ? 255 : 0;
                printf("%u", value);
            }
        }
        printf("]}");
    }
    puts("]}");
    free(bytes);
    return 0;
}
