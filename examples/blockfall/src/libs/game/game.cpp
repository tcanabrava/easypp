#include "game.hpp"

#include <algorithm>
#include <array>
#include <cmath>

namespace blockfall {

namespace {

constexpr double lock_delay = 0.5;
constexpr int max_lock_resets = 15;
constexpr int preview_count = 3;
constexpr std::array<int, 5> line_scores = {0, 100, 300, 500, 800};
constexpr int spawn_x = 3;

}

Game::Game(std::uint32_t seed, Board start)
    : bag_(seed)
    , board_(start)
{
    for (int i = 0; i < preview_count; ++i) {
        queue_.push_back(bag_.next());
    }
    spawn(take_next());
}

Shape Game::take_next()
{
    const Shape shape = queue_.front();
    queue_.pop_front();
    queue_.push_back(bag_.next());
    return shape;
}

void Game::spawn(Shape shape)
{
    current_ = Piece{shape, 0, spawn_x, 0};
    gravity_timer_ = 0;
    lock_timer_ = 0;
    lock_resets_ = 0;
    if (!board_.fits(current_)) {
        over_ = true;
    }
}

bool Game::try_move(int dx, int dy)
{
    Piece moved = current_;
    moved.x += dx;
    moved.y += dy;
    if (!board_.fits(moved)) {
        return false;
    }
    current_ = moved;
    return true;
}

bool Game::try_rotate(int direction)
{
    if (current_.shape == Shape::O) {
        return false;
    }
    // Try in place, then nudged sideways or up, so pieces can rotate next to walls.
    constexpr std::array<Cell, 6> kicks = {{{0, 0}, {-1, 0}, {1, 0}, {-2, 0}, {2, 0}, {0, -1}}};
    for (Cell kick : kicks) {
        Piece rotated = current_;
        rotated.rotation = (rotated.rotation + direction + 4) % 4;
        rotated.x += kick.x;
        rotated.y += kick.y;
        if (board_.fits(rotated)) {
            current_ = rotated;
            return true;
        }
    }
    return false;
}

bool Game::resting() const
{
    Piece below = current_;
    below.y += 1;
    return !board_.fits(below);
}

void Game::moved_while_resting()
{
    // Moving a resting piece buys a little time, but not forever.
    if (resting() && lock_resets_ < max_lock_resets) {
        lock_timer_ = 0;
        ++lock_resets_;
    }
}

void Game::handle(Input input)
{
    if (over_) {
        return;
    }
    switch (input) {
    case Input::Left:
        if (try_move(-1, 0)) {
            moved_while_resting();
        }
        break;
    case Input::Right:
        if (try_move(1, 0)) {
            moved_while_resting();
        }
        break;
    case Input::SoftDrop:
        if (try_move(0, 1)) {
            score_ += 1;
            gravity_timer_ = 0;
        }
        break;
    case Input::HardDrop: {
        int rows = 0;
        while (try_move(0, 1)) {
            ++rows;
        }
        score_ += 2 * rows;
        lock_piece();
        break;
    }
    case Input::RotateCw:
        if (try_rotate(1)) {
            moved_while_resting();
        }
        break;
    case Input::RotateCcw:
        if (try_rotate(-1)) {
            moved_while_resting();
        }
        break;
    case Input::Hold:
        hold();
        break;
    }
}

void Game::hold()
{
    if (!can_hold_) {
        return;
    }
    const Shape current = current_.shape;
    if (held_) {
        const Shape swapped = *held_;
        held_ = current;
        spawn(swapped);
    } else {
        held_ = current;
        spawn(take_next());
    }
    can_hold_ = false;
}

void Game::lock_piece()
{
    board_.lock(current_);
    const int cleared = board_.clear_lines();
    lines_ += cleared;
    score_ += line_scores[cleared] * level_;
    level_ = 1 + lines_ / 10;
    can_hold_ = true;
    spawn(take_next());
}

void Game::tick(double seconds)
{
    if (over_) {
        return;
    }
    if (resting()) {
        lock_timer_ += seconds;
        if (lock_timer_ >= lock_delay) {
            lock_piece();
        }
        return;
    }
    lock_timer_ = 0;
    gravity_timer_ += seconds;
    const double interval = gravity_interval();
    while (gravity_timer_ >= interval) {
        gravity_timer_ -= interval;
        if (!try_move(0, 1)) {
            gravity_timer_ = 0;
            break;
        }
    }
}

double Game::gravity_interval() const
{
    return std::max(0.05, 0.8 * std::pow(0.85, level_ - 1));
}

Piece Game::ghost() const
{
    Piece landing = current_;
    while (true) {
        Piece below = landing;
        below.y += 1;
        if (!board_.fits(below)) {
            return landing;
        }
        landing = below;
    }
}

}
