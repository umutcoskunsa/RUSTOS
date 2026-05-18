#include <stdio.h>
#include <stdlib.h>

int main() {
    printf("Hello from C world on MYNEWOS!\n");
    
    void *ptr = malloc(1024);
    if (ptr) {
        printf("Malloc works! ptr = %lx\n", (unsigned long)ptr);
        free(ptr);
        printf("Free works too!\n");
    } else {
        printf("Malloc FAILED!\n");
    }

    printf("Exiting now...\n");
    return 0;
}
