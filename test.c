#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

int main() {
    int32_t counter = 50;

    while (1) {
        system("cls");
        printf("current value: %i\n", counter);

        char input;
        scanf(" %c", &input);

        if (input == '+') {
            counter++;
        } else if (input == '-') {
            counter--;
        }
    }

    return 0;
}
