// Blockfall: an original falling-block puzzle game.
//
// Controls: Left/Right move, Up or X rotate clockwise, Z rotate
// counter-clockwise, Down soft drop, Space hard drop, C or Shift hold,
// P pause, R restart after game over, Esc quit.
//
// Testing aids: `--frames N` quits after N frames, `--demo` makes the game
// play itself, `--screenshot FILE.bmp` saves the last frame. For example:
//   SDL_VIDEODRIVER=dummy blockfall --demo --frames 600 --screenshot shot.bmp

#include "render/render.hpp"

#include <SDL2/SDL.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <random>

namespace {

std::uint32_t fresh_seed()
{
    std::random_device device;
    return device();
}

void save_screenshot(SDL_Renderer* renderer, const char* path)
{
    SDL_Surface* surface = SDL_CreateRGBSurfaceWithFormat(0, blockfall::window_width, blockfall::window_height, 32,
                                                          SDL_PIXELFORMAT_ARGB8888);
    if (surface == nullptr) {
        return;
    }
    if (SDL_RenderReadPixels(renderer, nullptr, SDL_PIXELFORMAT_ARGB8888, surface->pixels, surface->pitch) == 0
        && SDL_SaveBMP(surface, path) == 0) {
        std::printf("saved %s\n", path);
    } else {
        std::fprintf(stderr, "screenshot failed: %s\n", SDL_GetError());
    }
    SDL_FreeSurface(surface);
}

void press(blockfall::Game& game, bool& paused, bool& running, SDL_Keycode key)
{
    using blockfall::Input;
    if (key == SDLK_ESCAPE) {
        running = false;
        return;
    }
    if (key == SDLK_r && game.over()) {
        game = blockfall::Game(fresh_seed());
        paused = false;
        return;
    }
    if (key == SDLK_p && !game.over()) {
        paused = !paused;
        return;
    }
    if (paused) {
        return;
    }
    switch (key) {
    case SDLK_LEFT: game.handle(Input::Left); break;
    case SDLK_RIGHT: game.handle(Input::Right); break;
    case SDLK_DOWN: game.handle(Input::SoftDrop); break;
    case SDLK_SPACE: game.handle(Input::HardDrop); break;
    case SDLK_UP:
    case SDLK_x: game.handle(Input::RotateCw); break;
    case SDLK_z: game.handle(Input::RotateCcw); break;
    case SDLK_c:
    case SDLK_LSHIFT:
    case SDLK_RSHIFT: game.handle(Input::Hold); break;
    default: break;
    }
}

}

int main(int argc, char* argv[])
{
    long max_frames = -1;
    bool demo = false;
    const char* screenshot = nullptr;
    for (int i = 1; i < argc; ++i) {
        if (std::strcmp(argv[i], "--frames") == 0 && i + 1 < argc) {
            max_frames = std::strtol(argv[++i], nullptr, 10);
        } else if (std::strcmp(argv[i], "--screenshot") == 0 && i + 1 < argc) {
            screenshot = argv[++i];
        } else if (std::strcmp(argv[i], "--demo") == 0) {
            demo = true;
        }
    }

    if (SDL_Init(SDL_INIT_VIDEO) != 0) {
        std::fprintf(stderr, "SDL_Init failed: %s\n", SDL_GetError());
        return 1;
    }
    SDL_Window* window = SDL_CreateWindow("Blockfall", SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED,
                                          blockfall::window_width, blockfall::window_height, 0);
    if (window == nullptr) {
        std::fprintf(stderr, "SDL_CreateWindow failed: %s\n", SDL_GetError());
        SDL_Quit();
        return 1;
    }
    SDL_Renderer* renderer = SDL_CreateRenderer(window, -1, SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC);
    if (renderer == nullptr) {
        renderer = SDL_CreateRenderer(window, -1, SDL_RENDERER_SOFTWARE);
    }
    if (renderer == nullptr) {
        std::fprintf(stderr, "SDL_CreateRenderer failed: %s\n", SDL_GetError());
        SDL_DestroyWindow(window);
        SDL_Quit();
        return 1;
    }
    SDL_RendererInfo info{};
    SDL_GetRendererInfo(renderer, &info);
    const bool vsync = (info.flags & SDL_RENDERER_PRESENTVSYNC) != 0;

    blockfall::Game game(demo ? 2024 : fresh_seed());
    std::mt19937 demo_moves(7);
    bool paused = false;
    bool running = true;
    long frames = 0;
    Uint64 previous = SDL_GetPerformanceCounter();

    while (running) {
        SDL_Event event;
        while (SDL_PollEvent(&event)) {
            if (event.type == SDL_QUIT) {
                running = false;
            } else if (event.type == SDL_KEYDOWN) {
                press(game, paused, running, event.key.keysym.sym);
            } else if (event.type == SDL_WINDOWEVENT && event.window.event == SDL_WINDOWEVENT_FOCUS_LOST && !demo
                       && !game.over()) {
                paused = true;
            }
        }

        const Uint64 now = SDL_GetPerformanceCounter();
        const double seconds = static_cast<double>(now - previous) / static_cast<double>(SDL_GetPerformanceFrequency());
        previous = now;
        if (demo && frames % 12 == 0) {
            // Shift and turn a little at random, then drop.
            using blockfall::Input;
            constexpr Input moves[] = {Input::Left, Input::Right, Input::RotateCw, Input::HardDrop};
            game.handle(moves[demo_moves() % 4]);
        }
        if (!paused) {
            game.tick(demo ? 1.0 / 60 : seconds);
        }

        blockfall::draw(renderer, game, paused);
        const bool last = max_frames >= 0 && frames + 1 >= max_frames;
        if (last && screenshot != nullptr) {
            save_screenshot(renderer, screenshot);
        }
        SDL_RenderPresent(renderer);
        if (!vsync && !demo) {
            SDL_Delay(16);
        }

        if (max_frames >= 0 && ++frames >= max_frames) {
            running = false;
        }
    }

    std::printf("final score %d, lines %d, level %d\n", game.score(), game.lines(), game.level());
    SDL_DestroyRenderer(renderer);
    SDL_DestroyWindow(window);
    SDL_Quit();
    return 0;
}
