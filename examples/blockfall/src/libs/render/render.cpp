#include "render.hpp"

#include "pixelfont/pixelfont.hpp"

#include <SDL2/SDL.h>

#include <algorithm>
#include <array>
#include <string>

namespace blockfall {

namespace {

constexpr int cell = 30;
constexpr int well_x = 20;
constexpr int well_y = 20;
constexpr int panel_x = well_x + board_width * cell + 24;
constexpr int preview_cell = 18;

constexpr SDL_Color background{22, 24, 31, 255};
constexpr SDL_Color well{14, 15, 20, 255};
constexpr SDL_Color grid_line{30, 32, 41, 255};
constexpr SDL_Color label{120, 126, 150, 255};
constexpr SDL_Color value{236, 238, 245, 255};

// One colour per shape.
constexpr std::array<SDL_Color, shape_count> palette = {{
    {239, 131, 110, 255}, // I: coral
    {126, 214, 184, 255}, // O: mint
    {240, 200, 96, 255},  // T: gold
    {112, 178, 236, 255}, // S: sky
    {190, 150, 230, 255}, // Z: lilac
    {246, 176, 140, 255}, // J: peach
    {160, 196, 120, 255}, // L: sage
}};

SDL_Color color_of(Shape shape)
{
    return palette[static_cast<int>(shape)];
}

SDL_Color scaled(SDL_Color c, int percent)
{
    auto f = [percent](Uint8 v) { return static_cast<Uint8>(std::min(255, v * percent / 100)); };
    return SDL_Color{f(c.r), f(c.g), f(c.b), c.a};
}

void fill(SDL_Renderer* r, int x, int y, int w, int h, SDL_Color c)
{
    SDL_SetRenderDrawColor(r, c.r, c.g, c.b, c.a);
    const SDL_Rect rect{x, y, w, h};
    SDL_RenderFillRect(r, &rect);
}

void outline(SDL_Renderer* r, int x, int y, int w, int h, SDL_Color c)
{
    SDL_SetRenderDrawColor(r, c.r, c.g, c.b, c.a);
    const SDL_Rect rect{x, y, w, h};
    SDL_RenderDrawRect(r, &rect);
}

// A block: flat face, a lighter top edge and a darker bottom edge.
void block(SDL_Renderer* r, int x, int y, int size, SDL_Color c)
{
    const int edge = std::max(2, size / 8);
    fill(r, x + 1, y + 1, size - 2, size - 2, c);
    fill(r, x + 1, y + 1, size - 2, edge, scaled(c, 125));
    fill(r, x + 1, y + size - 1 - edge, size - 2, edge, scaled(c, 70));
}

void draw_piece(SDL_Renderer* r, Shape shape, int rotation, int origin_x, int origin_y, int size)
{
    for (Cell c : cells(shape, rotation)) {
        block(r, origin_x + c.x * size, origin_y + c.y * size, size, color_of(shape));
    }
}

// Draws a shape centred in a w x h box (used for the next/hold previews).
void draw_centered(SDL_Renderer* r, Shape shape, int x, int y, int w, int h)
{
    int min_x = 4, max_x = 0, min_y = 4, max_y = 0;
    for (Cell c : cells(shape, 0)) {
        min_x = std::min(min_x, c.x);
        max_x = std::max(max_x, c.x);
        min_y = std::min(min_y, c.y);
        max_y = std::max(max_y, c.y);
    }
    const int pw = (max_x - min_x + 1) * preview_cell;
    const int ph = (max_y - min_y + 1) * preview_cell;
    const int ox = x + (w - pw) / 2 - min_x * preview_cell;
    const int oy = y + (h - ph) / 2 - min_y * preview_cell;
    draw_piece(r, shape, 0, ox, oy, preview_cell);
}

void stat(SDL_Renderer* r, int y, const char* name, int number)
{
    pixelfont::draw_text(r, panel_x, y, 3, name, label);
    pixelfont::draw_text(r, panel_x, y + 24, 4, std::to_string(number), value);
}

void banner(SDL_Renderer* r, const char* title, const char* hint)
{
    fill(r, well_x, well_y, board_width * cell, board_height * cell, SDL_Color{0, 0, 0, 170});
    const int center = well_x + board_width * cell / 2;
    const int y = well_y + board_height * cell / 2 - 40;
    pixelfont::draw_text(r, center - pixelfont::text_width(title, 5) / 2, y, 5, title, value);
    pixelfont::draw_text(r, center - pixelfont::text_width(hint, 3) / 2, y + 50, 3, hint, label);
}

}

void draw(SDL_Renderer* r, const Game& game, bool paused)
{
    SDL_SetRenderDrawBlendMode(r, SDL_BLENDMODE_BLEND);
    fill(r, 0, 0, window_width, window_height, background);

    // The well and its grid.
    fill(r, well_x, well_y, board_width * cell, board_height * cell, well);
    for (int x = 1; x < board_width; ++x) {
        fill(r, well_x + x * cell, well_y, 1, board_height * cell, grid_line);
    }
    for (int y = 1; y < board_height; ++y) {
        fill(r, well_x, well_y + y * cell, board_width * cell, 1, grid_line);
    }

    // Settled blocks.
    for (int y = 0; y < board_height; ++y) {
        for (int x = 0; x < board_width; ++x) {
            if (auto shape = game.board().at(x, y)) {
                block(r, well_x + x * cell, well_y + y * cell, cell, scaled(color_of(*shape), 85));
            }
        }
    }

    if (!game.over()) {
        // Ghost: outlines where the piece will land.
        const Piece ghost = game.ghost();
        SDL_Color ghost_color = color_of(ghost.shape);
        ghost_color.a = 110;
        for (Cell c : cells(ghost.shape, ghost.rotation)) {
            const int y = ghost.y + c.y;
            if (y >= 0) {
                const int px = well_x + (ghost.x + c.x) * cell;
                const int py = well_y + y * cell;
                outline(r, px + 2, py + 2, cell - 4, cell - 4, ghost_color);
            }
        }

        const Piece& piece = game.current();
        for (Cell c : cells(piece.shape, piece.rotation)) {
            const int y = piece.y + c.y;
            if (y >= 0) {
                block(r, well_x + (piece.x + c.x) * cell, well_y + y * cell, cell, color_of(piece.shape));
            }
        }
    }
    outline(r, well_x - 1, well_y - 1, board_width * cell + 2, board_height * cell + 2, grid_line);

    // Side panel.
    constexpr int box_w = 150;
    pixelfont::draw_text(r, panel_x, well_y, 3, "NEXT", label);
    int y = well_y + 24;
    for (Shape s : game.upcoming()) {
        draw_centered(r, s, panel_x, y, box_w, 54);
        y += 58;
    }

    y += 12;
    pixelfont::draw_text(r, panel_x, y, 3, "HOLD", label);
    fill(r, panel_x, y + 24, box_w, 64, well);
    if (auto held = game.held()) {
        draw_centered(r, *held, panel_x, y + 24, box_w, 64);
    }

    stat(r, y + 112, "SCORE", game.score());
    stat(r, y + 182, "LINES", game.lines());
    stat(r, y + 252, "LEVEL", game.level());

    if (game.over()) {
        banner(r, "GAME OVER", "PRESS R");
    } else if (paused) {
        banner(r, "PAUSED", "PRESS P");
    }
}

}
