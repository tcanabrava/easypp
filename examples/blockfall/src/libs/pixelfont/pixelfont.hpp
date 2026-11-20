#pragma once

// A tiny 3x5 bitmap font: digits and the capital letters the game needs.
// Header-only on purpose: it shows a library with no .cpp files whose header
// pulls in a system library (SDL2), which easypp follows on its own.

#include <SDL2/SDL.h>

#include <array>
#include <string_view>

namespace pixelfont {

inline constexpr int glyph_width = 3;
inline constexpr int glyph_height = 5;

using Glyph = std::array<const char*, glyph_height>;

inline Glyph glyph(char c)
{
    switch (c) {
    case '0':
    case 'O':
        return {"###", "#.#", "#.#", "#.#", "###"};
    case '1':
        return {".#.", "##.", ".#.", ".#.", "###"};
    case '2':
        return {"###", "..#", "###", "#..", "###"};
    case '3':
        return {"###", "..#", ".##", "..#", "###"};
    case '4':
        return {"#.#", "#.#", "###", "..#", "..#"};
    case '5':
    case 'S':
        return {"###", "#..", "###", "..#", "###"};
    case '6':
        return {"###", "#..", "###", "#.#", "###"};
    case '7':
        return {"###", "..#", "..#", ".#.", ".#."};
    case '8':
        return {"###", "#.#", "###", "#.#", "###"};
    case '9':
        return {"###", "#.#", "###", "..#", "###"};
    case 'A':
        return {".#.", "#.#", "###", "#.#", "#.#"};
    case 'B':
        return {"##.", "#.#", "##.", "#.#", "##."};
    case 'C':
        return {"###", "#..", "#..", "#..", "###"};
    case 'D':
        return {"##.", "#.#", "#.#", "#.#", "##."};
    case 'E':
        return {"###", "#..", "##.", "#..", "###"};
    case 'F':
        return {"###", "#..", "##.", "#..", "#.."};
    case 'G':
        return {"###", "#..", "#.#", "#.#", "###"};
    case 'H':
        return {"#.#", "#.#", "###", "#.#", "#.#"};
    case 'I':
        return {"###", ".#.", ".#.", ".#.", "###"};
    case 'K':
        return {"#.#", "#.#", "##.", "#.#", "#.#"};
    case 'L':
        return {"#..", "#..", "#..", "#..", "###"};
    case 'M':
        return {"#.#", "###", "###", "#.#", "#.#"};
    case 'N':
        return {"##.", "#.#", "#.#", "#.#", "#.#"};
    case 'P':
        return {"###", "#.#", "###", "#..", "#.."};
    case 'R':
        return {"##.", "#.#", "##.", "#.#", "#.#"};
    case 'T':
        return {"###", ".#.", ".#.", ".#.", ".#."};
    case 'U':
        return {"#.#", "#.#", "#.#", "#.#", "###"};
    case 'V':
        return {"#.#", "#.#", "#.#", "#.#", ".#."};
    case 'W':
        return {"#.#", "#.#", "###", "###", "#.#"};
    case 'X':
        return {"#.#", "#.#", ".#.", "#.#", "#.#"};
    case 'Y':
        return {"#.#", "#.#", ".#.", ".#.", ".#."};
    default:
        return {"...", "...", "...", "...", "..."};
    }
}

inline int text_width(std::string_view text, int scale)
{
    if (text.empty()) {
        return 0;
    }
    return static_cast<int>(text.size()) * (glyph_width + 1) * scale - scale;
}

inline void draw_text(SDL_Renderer* renderer, int x, int y, int scale, std::string_view text, SDL_Color color)
{
    SDL_SetRenderDrawColor(renderer, color.r, color.g, color.b, color.a);
    for (char c : text) {
        const Glyph g = glyph(c);
        for (int row = 0; row < glyph_height; ++row) {
            for (int col = 0; col < glyph_width; ++col) {
                if (g[row][col] == '#') {
                    const SDL_Rect pixel {x + col * scale, y + row * scale, scale, scale};
                    SDL_RenderFillRect(renderer, &pixel);
                }
            }
        }
        x += (glyph_width + 1) * scale;
    }
}

}
