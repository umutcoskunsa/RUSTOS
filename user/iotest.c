#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main() {
    printf("Testing File I/O with lseek...\n");

    FILE *f = fopen("readme.txt", "r");
    if (!f) {
        printf("Failed to open readme.txt!\n");
        return 1;
    }

    char buf[64];
    size_t n = fread(buf, 1, 15, f);
    buf[n] = '\0';
    printf("Read first 15 bytes: '%s'\n", buf);

    printf("Seeking to offset 10...\n");
    fseek(f, 10, SEEK_SET);
    
    n = fread(buf, 1, 15, f);
    buf[n] = '\0';
    printf("Read 15 bytes from offset 10: '%s'\n", buf);

    long pos = ftell(f);
    printf("Current position: %ld\n", pos);

    fclose(f);
    printf("Test complete.\n");

    return 0;
}
