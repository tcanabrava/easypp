#pragma once

#include "game/game.hpp"

// Only a forward declaration: code that uses this header does not see SDL,
// yet easypp still links SDL2 into it because render.cpp needs it.
struct SDL_Renderer;

namespace blockfall {

inline constexpr int window_width = 540;
inline constexpr int window_height = 640;

void draw(SDL_Renderer* renderer, const Game& game, bool paused);

}
