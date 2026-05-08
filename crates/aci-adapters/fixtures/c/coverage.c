#include <stdio.h>

struct Point {
    int x;
    int y;
};

int add(int left, int right) {
    return left + right;
}

int main(void) {
    printf("%d\n", add(1, 2));
    return 0;
}
