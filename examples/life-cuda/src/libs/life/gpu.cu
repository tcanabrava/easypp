#include "life.hpp"

#include <cuda_runtime.h>

namespace life {
namespace {

__global__ void evolve(const unsigned char* current, unsigned char* next)
{
    const int x = static_cast<int>(blockIdx.x * blockDim.x + threadIdx.x);
    const int y = static_cast<int>(blockIdx.y * blockDim.y + threadIdx.y);
    if (x >= width || y >= height) {
        return;
    }

    int neighbours = 0;
    for (int dy = -1; dy <= 1; ++dy) {
        for (int dx = -1; dx <= 1; ++dx) {
            const int nx = (x + dx + width) % width;
            const int ny = (y + dy + height) % height;
            neighbours += current[ny * width + nx];
        }
    }
    const int index = y * width + x;
    const bool alive = current[index] != 0;
    neighbours -= alive;
    next[index] = neighbours == 3 || (alive && neighbours == 2);
}

}

CudaLife::CudaLife() noexcept
{
    auto result = cudaMalloc(&current_, Board().size());
    if (result == cudaSuccess) {
        result = cudaMalloc(&next_, Board().size());
    }
    if (result != cudaSuccess) {
        error_ = cudaGetErrorString(result);
    }
}

CudaLife::~CudaLife()
{
    if (current_) {
        cudaFree(current_);
    }
    if (next_) {
        cudaFree(next_);
    }
}

const char* CudaLife::step(Board& board) noexcept
{
    if (error_) {
        return error_;
    }

    auto result = cudaMemcpy(current_, board.data(), board.size(), cudaMemcpyHostToDevice);
    if (result == cudaSuccess) {
        const dim3 block(16, 16);
        const dim3 grid((width + block.x - 1) / block.x, (height + block.y - 1) / block.y);
        evolve<<<grid, block>>>(current_, next_);
        result = cudaGetLastError();
    }
    if (result == cudaSuccess) {
        result = cudaMemcpy(board.data(), next_, board.size(), cudaMemcpyDeviceToHost);
    }
    return result == cudaSuccess ? nullptr : cudaGetErrorString(result);
}

}
