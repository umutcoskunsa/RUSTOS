#include <stdio.h>
#include <unistd.h>

int main() {
    printf("Keyboard Input Test\n");
    printf("Press keys to see scancodes. Press 'q' (scancode 0x10) to exit.\n");

    while(1) {
        int key = getkey();
        if (key > 0) {
            printf("Scancode: 0x%x\n", key);
            if (key == 0x10) { // 'q' pressed
                printf("Exiting...\n");
                break;
            }
        }
        // Small delay to avoid pegging the CPU too hard
        for(volatile int i=0; i<1000000; i++);
    }

    return 0;
}
