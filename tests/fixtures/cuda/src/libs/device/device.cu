#include "device.hpp"
#include "answer.cuh"
#include <cuda_runtime.h>

namespace {
__global__ void write_answer(int* output)
{
    *output = answer;
}
}

int runtime_version()
{
    int version = 0;
    return cudaRuntimeGetVersion(&version) == cudaSuccess ? version : 0;
}

bool write_answer_on_gpu()
{
    int* output = nullptr;
    if (cudaMalloc(&output, sizeof(int)) != cudaSuccess) {
        return false;
    }
    write_answer<<<1, 1>>>(output);
    int value = 0;
    const auto result = cudaMemcpy(&value, output, sizeof(value), cudaMemcpyDeviceToHost);
    return cudaFree(output) == cudaSuccess && result == cudaSuccess && value == answer;
}
