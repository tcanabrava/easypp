// Two beginner mistakes: reading an uninitialised variable, and a
// variable-length array (a compiler extension, not standard C++).
#include <cstdio>
#include <cstdlib>

int main(int argc, char*[])
{
    int total;
    for (int i = 0; i < argc; ++i) {
        total += i;
    }
    int scores[argc];
    scores[0] = total;
    std::printf("__cplusplus = %ld, score = %d\n", __cplusplus, scores[0]);
    return EXIT_SUCCESS;
}
