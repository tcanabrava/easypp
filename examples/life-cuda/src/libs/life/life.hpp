#pragma once

#include <array>

namespace life {

inline constexpr int width = 128;
inline constexpr int height = 96;
using Board = std::array<unsigned char, width * height>;

class CudaLife {
public:
    CudaLife() noexcept;
    ~CudaLife();

    CudaLife(const CudaLife&) = delete;
    CudaLife& operator=(const CudaLife&) = delete;

    const char* error() const noexcept { return error_; }
    const char* step(Board& board) noexcept;

private:
    unsigned char* current_ = nullptr;
    unsigned char* next_ = nullptr;
    const char* error_ = nullptr;
};

Board seeded_board();
int population(const Board& board);
Board reference_step(const Board& board);

}
