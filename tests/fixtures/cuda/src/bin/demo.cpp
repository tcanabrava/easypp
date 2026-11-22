#include "device/device.hpp"

#include <cstdio>
#include <string_view>

int main(int argc, char** argv)
{
    if (argc > 1 && std::string_view(argv[1]) == "--gpu") {
        if (!write_answer_on_gpu()) {
            std::fputs("CUDA kernel execution failed\n", stderr);
            return 1;
        }
        std::puts("CUDA kernel wrote 42");
    }
    const int version = runtime_version();
    std::printf("CUDA runtime %d\n", version);
    return version > 0 ? 0 : 1;
}
